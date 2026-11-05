use std::{
    fmt, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
        Arc,
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result};

use crate::application::{
    ConflictResolution, FileOperationKind, LocalOperationRequest, OperationConflict,
    OperationError, OperationEvent, OperationSnapshot, OperationSummary, PathAccess, TaskSpawner,
};

#[derive(Clone)]
pub struct OperationHandle {
    cancelled: Arc<AtomicBool>,
    resolution_tx: Sender<ConflictResolution>,
}

impl OperationHandle {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn resolve_conflict(&self, resolution: ConflictResolution) {
        let _ = self.resolution_tx.send(resolution);
    }
}

#[derive(Clone, Debug)]
struct OperationPlan {
    total_bytes: u64,
    total_entries: u64,
}

#[derive(Clone, Debug)]
struct CopyProgress {
    processed_bytes: u64,
    processed_entries: u64,
}

/// One step of an operation. As the context of a failure it keeps the path and
/// the kind of access, so that a refusal can be named to the user.
#[derive(Debug)]
struct Step {
    access: PathAccess,
    path: PathBuf,
    what: &'static str,
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.what, self.path.display())
    }
}

fn step(access: PathAccess, path: &Path, what: &'static str) -> Step {
    Step {
        access,
        path: path.to_path_buf(),
        what,
    }
}

/// The failure as the UI gets it. The alternate form keeps the cause, which is
/// the part that says why; the plain one ends at the outermost context.
fn operation_failure(error: &anyhow::Error) -> OperationError {
    let detail = format!("{error:#}");
    match error.downcast_ref::<Step>() {
        Some(step) if is_refusal(error) => {
            OperationError::access_denied(step.access, step.path.clone(), detail)
        }
        _ => OperationError::execution(detail),
    }
}

fn is_refusal(error: &anyhow::Error) -> bool {
    error
        .chain()
        .filter_map(|cause| cause.downcast_ref::<io::Error>())
        .any(|cause| cause.kind() == io::ErrorKind::PermissionDenied)
}

pub fn start_operation(
    task_spawner: TaskSpawner,
    request: LocalOperationRequest,
) -> (OperationHandle, Receiver<OperationEvent>) {
    let (tx, rx) = mpsc::channel();
    let (resolution_tx, resolution_rx) = mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    let handle = OperationHandle {
        cancelled: Arc::clone(&cancelled),
        resolution_tx,
    };

    task_spawner.spawn(move || {
        let result = match request.kind {
            FileOperationKind::Copy | FileOperationKind::Move => {
                run_transfer(request.clone(), &tx, &cancelled, &resolution_rx)
            }
            FileOperationKind::Delete => run_delete(request.clone(), &tx, &cancelled),
        };

        if let Err(error) = result {
            let _ = tx.send(OperationEvent::Failed(operation_failure(&error)));
        }
    });

    (handle, rx)
}

/// Repeats a local operation through the Windows shell, which asks for
/// administrator rights first. Windows shows its own progress and conflict
/// dialogs, so only the outcome comes back here.
#[cfg(target_os = "windows")]
pub fn start_elevated_operation(
    task_spawner: TaskSpawner,
    request: LocalOperationRequest,
    overwrite_silently: bool,
    owner: isize,
) -> (OperationHandle, Receiver<OperationEvent>) {
    use crate::platform::windows_shell_operation::{perform, ShellOutcome};

    let (tx, rx) = mpsc::channel();
    // The dialogs belong to Windows, so nothing here can cancel or resolve.
    let (resolution_tx, _) = mpsc::channel();
    let handle = OperationHandle {
        cancelled: Arc::new(AtomicBool::new(false)),
        resolution_tx,
    };

    task_spawner.spawn(move || {
        let started_at = Instant::now();
        // Measured first: a move or delete leaves nothing to measure afterwards.
        let plan = build_plan_for_paths(&request.sources).unwrap_or(OperationPlan {
            total_bytes: 0,
            total_entries: request.sources.len() as u64,
        });
        let outcome = shell_operation(&request, overwrite_silently, owner)
            .and_then(|operation| perform(&operation));

        let finished = |bytes, entries| {
            summary(
                request.kind.clone(),
                request.sources.clone(),
                request.target_directory.clone(),
                bytes,
                entries,
                started_at,
            )
        };
        let _ = tx.send(match outcome {
            Ok(ShellOutcome::Completed) => {
                OperationEvent::Finished(finished(plan.total_bytes, plan.total_entries))
            }
            Ok(ShellOutcome::Aborted) => OperationEvent::Cancelled(finished(0, 0)),
            Err(error) => OperationEvent::Failed(OperationError::execution(format!("{error:#}"))),
        });
    });

    (handle, rx)
}

#[cfg(target_os = "windows")]
fn shell_operation(
    request: &LocalOperationRequest,
    overwrite_silently: bool,
    owner: isize,
) -> Result<crate::platform::windows_shell_operation::ShellOperation> {
    use crate::platform::windows_shell_operation::{ShellAction, ShellOperation, ShellTarget};

    let target = || -> Result<ShellTarget> {
        let directory = request
            .target_directory
            .clone()
            .context("No target directory set for file operation")?;
        // An explicit target names the one source, as in `run_transfer`.
        let renamed = request
            .target_path
            .as_deref()
            .filter(|_| request.sources.len() == 1);
        Ok(ShellTarget {
            directory: renamed
                .and_then(Path::parent)
                .map_or(directory, Path::to_path_buf),
            name: renamed
                .and_then(Path::file_name)
                .map(std::ffi::OsStr::to_os_string),
            overwrite_silently,
        })
    };

    Ok(ShellOperation {
        action: match request.kind {
            FileOperationKind::Copy => ShellAction::Copy(target()?),
            FileOperationKind::Move => ShellAction::Move(target()?),
            FileOperationKind::Delete => ShellAction::Delete {
                recycle: request.use_recycle_bin,
            },
        },
        sources: request.sources.clone(),
        elevate: true,
        owner,
    })
}

fn run_transfer(
    request: LocalOperationRequest,
    tx: &mpsc::Sender<OperationEvent>,
    cancelled: &Arc<AtomicBool>,
    resolution_rx: &Receiver<ConflictResolution>,
) -> Result<()> {
    let target_directory = request
        .target_directory
        .clone()
        .context("No target directory set for file operation")?;
    let plan = build_plan_for_paths(&request.sources)?;
    let started_at = Instant::now();
    let mut progress = CopyProgress {
        processed_bytes: 0,
        processed_entries: 0,
    };
    let delete_source = matches!(request.kind, FileOperationKind::Move);

    let mut completed_targets: Vec<PathBuf> = Vec::new();
    for source in &request.sources {
        let source_name = source.file_name().context("Source path has no file name")?;
        let target_path = if request.sources.len() == 1 {
            request
                .target_path
                .clone()
                .unwrap_or_else(|| target_directory.join(source_name))
        } else {
            target_directory.join(source_name)
        };
        // A move copies first. Finding out afterwards would leave both behind.
        if delete_source {
            delete_access(source)
                .with_context(|| step(PathAccess::Delete, source, "Could not move"))?;
        }
        let transfer_result = copy_path(
            source,
            &target_path,
            &plan,
            &mut progress,
            started_at,
            tx,
            &request.kind,
            cancelled,
            resolution_rx,
        );

        if cancelled.load(Ordering::Relaxed) {
            let _ = cleanup_path(&target_path);
            for target in &completed_targets {
                let _ = cleanup_path(target);
            }
            let _ = tx.send(OperationEvent::Cancelled(summary(
                request.kind,
                request.sources,
                Some(target_directory),
                progress.processed_bytes,
                progress.processed_entries,
                started_at,
            )));
            return Ok(());
        }

        transfer_result?;

        if delete_source {
            cleanup_path(source).with_context(|| {
                step(PathAccess::Delete, source, "Could not remove moved source")
            })?;
        }
        completed_targets.push(target_path);
    }

    let _ = tx.send(OperationEvent::Finished(summary(
        request.kind,
        request.sources,
        Some(target_directory),
        plan.total_bytes,
        plan.total_entries,
        started_at,
    )));

    Ok(())
}

fn run_delete(
    request: LocalOperationRequest,
    tx: &mpsc::Sender<OperationEvent>,
    cancelled: &Arc<AtomicBool>,
) -> Result<()> {
    let plan = build_plan_for_paths(&request.sources)?;
    let started_at = Instant::now();
    let mut progress = CopyProgress {
        processed_bytes: 0,
        processed_entries: 0,
    };

    for source in &request.sources {
        if request.use_recycle_bin {
            delete_via_recycle_bin(
                source,
                &plan,
                &mut progress,
                started_at,
                tx,
                &request.kind,
                cancelled,
            )?;
        } else {
            delete_path(
                source,
                &plan,
                &mut progress,
                started_at,
                tx,
                &request.kind,
                cancelled,
            )?;
        }

        if cancelled.load(Ordering::Relaxed) {
            let _ = tx.send(OperationEvent::Cancelled(summary(
                request.kind,
                request.sources,
                None,
                progress.processed_bytes,
                progress.processed_entries,
                started_at,
            )));
            return Ok(());
        }
    }

    let _ = tx.send(OperationEvent::Finished(summary(
        request.kind,
        request.sources,
        None,
        plan.total_bytes,
        plan.total_entries,
        started_at,
    )));

    Ok(())
}

fn build_plan_for_paths(paths: &[PathBuf]) -> Result<OperationPlan> {
    let mut total = OperationPlan {
        total_bytes: 0,
        total_entries: 0,
    };

    for path in paths {
        let plan = build_copy_plan(path)?;
        total.total_bytes += plan.total_bytes;
        total.total_entries += plan.total_entries;
    }

    Ok(total)
}

fn build_copy_plan(path: &Path) -> Result<OperationPlan> {
    let metadata = fs::metadata(path)
        .with_context(|| step(PathAccess::Read, path, "Could not read metadata for"))?;

    if metadata.is_dir() {
        let mut plan = OperationPlan {
            total_bytes: 0,
            total_entries: 1,
        };

        for entry in fs::read_dir(path)
            .with_context(|| step(PathAccess::Read, path, "Could not read directory"))?
        {
            let entry =
                entry.with_context(|| step(PathAccess::Read, path, "Could not read directory"))?;
            let child_plan = build_copy_plan(&entry.path())?;
            plan.total_bytes += child_plan.total_bytes;
            plan.total_entries += child_plan.total_entries;
        }

        Ok(plan)
    } else {
        Ok(OperationPlan {
            total_bytes: metadata.len(),
            total_entries: 1,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn copy_path(
    source: &Path,
    target: &Path,
    plan: &OperationPlan,
    progress: &mut CopyProgress,
    started_at: Instant,
    tx: &mpsc::Sender<OperationEvent>,
    kind: &FileOperationKind,
    cancelled: &Arc<AtomicBool>,
    resolution_rx: &Receiver<ConflictResolution>,
) -> Result<()> {
    if cancelled.load(Ordering::Relaxed) {
        return Ok(());
    }

    let metadata = fs::metadata(source)
        .with_context(|| step(PathAccess::Read, source, "Could not read metadata for"))?;
    let Some(target) = resolve_target_conflict(
        source,
        target.to_path_buf(),
        plan,
        progress,
        started_at,
        tx,
        kind,
        cancelled,
        resolution_rx,
    )?
    else {
        return Ok(());
    };

    if metadata.is_dir() {
        fs::create_dir_all(&target)
            .with_context(|| step(PathAccess::Write, &target, "Could not create target folder"))?;

        progress.processed_entries += 1;
        send_progress(tx, kind.clone(), source, plan, progress, started_at);

        for entry in fs::read_dir(source)
            .with_context(|| step(PathAccess::Read, source, "Could not read directory"))?
        {
            let entry = entry
                .with_context(|| step(PathAccess::Read, source, "Could not read directory"))?;
            let child_source = entry.path();
            let child_target = target.join(entry.file_name());
            copy_path(
                &child_source,
                &child_target,
                plan,
                progress,
                started_at,
                tx,
                kind,
                cancelled,
                resolution_rx,
            )?;
            if cancelled.load(Ordering::Relaxed) {
                let _ = cleanup_path(&target);
                return Ok(());
            }
        }

        if cancelled.load(Ordering::Relaxed) {
            let _ = cleanup_path(&target);
        }

        return Ok(());
    }

    copy_file(
        source, &target, plan, progress, started_at, tx, kind, cancelled,
    )
}

#[allow(clippy::too_many_arguments)]
fn copy_file(
    source: &Path,
    target: &Path,
    plan: &OperationPlan,
    progress: &mut CopyProgress,
    started_at: Instant,
    tx: &mpsc::Sender<OperationEvent>,
    kind: &FileOperationKind,
    cancelled: &Arc<AtomicBool>,
) -> Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .with_context(|| step(PathAccess::Write, parent, "Could not create target folder"))?;
    }

    let mut reader = fs::File::open(source)
        .with_context(|| step(PathAccess::Read, source, "Could not open source file"))?;
    let mut writer = fs::File::create(target)
        .with_context(|| step(PathAccess::Write, target, "Could not create target file"))?;
    let mut buffer = vec![0_u8; 1024 * 1024];

    loop {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }

        let read = reader
            .read(&mut buffer)
            .with_context(|| step(PathAccess::Read, source, "Could not read"))?;
        if read == 0 {
            break;
        }

        writer
            .write_all(&buffer[..read])
            .with_context(|| step(PathAccess::Write, target, "Could not write"))?;
        writer
            .flush()
            .with_context(|| step(PathAccess::Write, target, "Could not flush"))?;
        progress.processed_bytes += read as u64;
        send_progress(tx, kind.clone(), source, plan, progress, started_at);
    }

    if cancelled.load(Ordering::Relaxed) {
        let _ = fs::remove_file(target);
        return Ok(());
    }

    progress.processed_entries += 1;
    send_progress(tx, kind.clone(), source, plan, progress, started_at);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn delete_path(
    path: &Path,
    plan: &OperationPlan,
    progress: &mut CopyProgress,
    started_at: Instant,
    tx: &mpsc::Sender<OperationEvent>,
    kind: &FileOperationKind,
    cancelled: &Arc<AtomicBool>,
) -> Result<()> {
    if cancelled.load(Ordering::Relaxed) {
        return Ok(());
    }

    let metadata = fs::metadata(path)
        .with_context(|| step(PathAccess::Read, path, "Could not read metadata for"))?;

    if metadata.is_dir() {
        for entry in fs::read_dir(path)
            .with_context(|| step(PathAccess::Read, path, "Could not read directory"))?
        {
            let entry =
                entry.with_context(|| step(PathAccess::Read, path, "Could not read directory"))?;
            delete_path(
                &entry.path(),
                plan,
                progress,
                started_at,
                tx,
                kind,
                cancelled,
            )?;
            if cancelled.load(Ordering::Relaxed) {
                return Ok(());
            }
        }

        fs::remove_dir(path)
            .with_context(|| step(PathAccess::Delete, path, "Could not delete folder"))?;
        progress.processed_entries += 1;
        send_progress(tx, kind.clone(), path, plan, progress, started_at);
        return Ok(());
    }

    let file_len = metadata.len();
    fs::remove_file(path)
        .with_context(|| step(PathAccess::Delete, path, "Could not delete file"))?;
    progress.processed_bytes += file_len;
    progress.processed_entries += 1;
    send_progress(tx, kind.clone(), path, plan, progress, started_at);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn delete_via_recycle_bin(
    path: &Path,
    plan: &OperationPlan,
    progress: &mut CopyProgress,
    started_at: Instant,
    tx: &mpsc::Sender<OperationEvent>,
    kind: &FileOperationKind,
    cancelled: &Arc<AtomicBool>,
) -> Result<()> {
    if cancelled.load(Ordering::Relaxed) {
        return Ok(());
    }

    let deleted = build_copy_plan(path)?;
    if let Err(error) = trash::delete(path) {
        // The recycle bin reports only that it gave up, not why.
        let cause = match delete_access(path) {
            Err(refusal) => anyhow::Error::from(refusal),
            Ok(()) => anyhow::Error::from(error),
        };
        return Err(cause.context(step(
            PathAccess::Delete,
            path,
            "Could not move to the recycle bin:",
        )));
    }
    progress.processed_bytes += deleted.total_bytes;
    progress.processed_entries += deleted.total_entries;
    send_progress(tx, kind.clone(), path, plan, progress, started_at);
    Ok(())
}

/// Asks the system whether `path` may be deleted, without touching it.
#[cfg(target_os = "windows")]
fn delete_access(path: &Path) -> io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt;

    use windows_sys::Win32::Storage::FileSystem::{
        DELETE, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    fs::OpenOptions::new()
        .access_mode(DELETE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        // Without it, Windows refuses to open a folder at all.
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
        .map(drop)
}

#[cfg(not(target_os = "windows"))]
fn delete_access(_path: &Path) -> io::Result<()> {
    Ok(())
}

fn cleanup_path(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let metadata = fs::metadata(path)?;
    if metadata.is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn resolve_target_conflict(
    source: &Path,
    mut target: PathBuf,
    plan: &OperationPlan,
    progress: &mut CopyProgress,
    started_at: Instant,
    tx: &mpsc::Sender<OperationEvent>,
    kind: &FileOperationKind,
    cancelled: &Arc<AtomicBool>,
    resolution_rx: &Receiver<ConflictResolution>,
) -> Result<Option<PathBuf>> {
    while target.exists() {
        let _ = tx.send(OperationEvent::Conflict(OperationConflict {
            kind: kind.clone(),
            source: source.to_path_buf(),
            target: target.clone(),
        }));

        match await_resolution(cancelled, resolution_rx) {
            ConflictResolution::Overwrite => {
                cleanup_path(&target).with_context(|| {
                    step(
                        PathAccess::Delete,
                        &target,
                        "Could not remove conflicting target",
                    )
                })?;
            }
            ConflictResolution::Skip => {
                let skipped = build_copy_plan(source)?;
                progress.processed_bytes += skipped.total_bytes;
                progress.processed_entries += skipped.total_entries;
                send_progress(tx, kind.clone(), source, plan, progress, started_at);
                return Ok(None);
            }
            ConflictResolution::Rename => {
                target = next_available_path(&target);
            }
            ConflictResolution::Cancel => {
                cancelled.store(true, Ordering::Relaxed);
                return Ok(None);
            }
        }
    }

    Ok(Some(target))
}

fn await_resolution(
    cancelled: &Arc<AtomicBool>,
    resolution_rx: &Receiver<ConflictResolution>,
) -> ConflictResolution {
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return ConflictResolution::Cancel;
        }

        match resolution_rx.recv_timeout(Duration::from_millis(150)) {
            Ok(resolution) => return resolution,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => return ConflictResolution::Cancel,
        }
    }
}

fn next_available_path(target: &Path) -> PathBuf {
    let parent = target.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = target
        .file_stem()
        .map(|value| value.to_string_lossy().into_owned())
        .or_else(|| {
            target
                .file_name()
                .map(|value| value.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "item".into());
    let extension = target
        .extension()
        .map(|value| value.to_string_lossy().into_owned());

    let mut index = 1usize;
    loop {
        let candidate_name = if index == 1 {
            format!("{stem} (copy)")
        } else {
            format!("{stem} (copy {index})")
        };
        let candidate = match &extension {
            Some(extension) if !extension.is_empty() => {
                parent.join(format!("{candidate_name}.{extension}"))
            }
            _ => parent.join(candidate_name),
        };

        if !candidate.exists() {
            return candidate;
        }
        index += 1;
    }
}

fn send_progress(
    tx: &mpsc::Sender<OperationEvent>,
    kind: FileOperationKind,
    current_item: &Path,
    plan: &OperationPlan,
    progress: &CopyProgress,
    started_at: Instant,
) {
    let _ = tx.send(OperationEvent::Progress(OperationSnapshot {
        kind,
        current_item: current_item.display().to_string(),
        processed_bytes: progress.processed_bytes,
        total_bytes: plan.total_bytes,
        processed_entries: progress.processed_entries,
        total_entries: plan.total_entries,
        started_at,
    }));
}

fn summary(
    kind: FileOperationKind,
    _sources: Vec<PathBuf>,
    _target: Option<PathBuf>,
    total_bytes: u64,
    total_entries: u64,
    started_at: Instant,
) -> OperationSummary {
    OperationSummary {
        kind,
        total_bytes,
        total_entries,
        elapsed: started_at.elapsed(),
    }
}

pub fn format_eta(snapshot: &OperationSnapshot) -> String {
    if snapshot.processed_bytes == 0 || snapshot.total_bytes <= snapshot.processed_bytes {
        return "ETA --:--".into();
    }

    let elapsed = snapshot.started_at.elapsed().as_secs_f64();
    if elapsed <= 0.0 {
        return "ETA --:--".into();
    }

    let bytes_per_second = snapshot.processed_bytes as f64 / elapsed;
    if bytes_per_second <= 0.0 {
        return "ETA --:--".into();
    }

    let remaining_seconds = ((snapshot.total_bytes - snapshot.processed_bytes) as f64
        / bytes_per_second)
        .round() as u64;
    let minutes = remaining_seconds / 60;
    let seconds = remaining_seconds % 60;
    format!("ETA {minutes:02}:{seconds:02}")
}

pub fn progress_percent(snapshot: &OperationSnapshot) -> f64 {
    if snapshot.total_bytes == 0 {
        return if snapshot.total_entries == 0 {
            0.0
        } else {
            (snapshot.processed_entries as f64 / snapshot.total_entries as f64).clamp(0.0, 1.0)
        };
    }

    (snapshot.processed_bytes as f64 / snapshot.total_bytes as f64).clamp(0.0, 1.0)
}

pub fn copy_entry(source: &Path, target: &Path) -> Result<()> {
    let metadata = fs::metadata(source)
        .with_context(|| format!("Could not read metadata for {}", source.display()))?;

    if metadata.is_dir() {
        fs::create_dir(target)
            .with_context(|| format!("Could not create directory {}", target.display()))?;
        for entry in fs::read_dir(source)
            .with_context(|| format!("Could not read directory {}", source.display()))?
        {
            let entry = entry?;
            copy_entry(&entry.path(), &target.join(entry.file_name()))?;
        }
        return Ok(());
    }

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Could not create target folder {}", parent.display()))?;
    }
    fs::copy(source, target).with_context(|| {
        format!(
            "Could not copy {} to {}",
            source.display(),
            target.display()
        )
    })?;
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/fs_operations_tests.rs"]
mod tests;
