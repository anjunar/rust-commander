use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};

use crate::application::{
    ConflictResolution, FileOperationKind, LocalOperationRequest, OperationConflict,
    OperationEvent, OperationSnapshot, OperationSummary,
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

pub fn start_operation(
    request: LocalOperationRequest,
) -> (OperationHandle, Receiver<OperationEvent>) {
    let (tx, rx) = mpsc::channel();
    let (resolution_tx, resolution_rx) = mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    let handle = OperationHandle {
        cancelled: Arc::clone(&cancelled),
        resolution_tx,
    };

    thread::spawn(move || {
        let result = match request.kind {
            FileOperationKind::Copy | FileOperationKind::Move => {
                run_transfer(request.clone(), &tx, &cancelled, &resolution_rx)
            }
            FileOperationKind::Delete => run_delete(request.clone(), &tx, &cancelled),
        };

        if let Err(error) = result {
            let _ = tx.send(OperationEvent::Failed(error.to_string()));
        }
    });

    (handle, rx)
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
        let target_path = target_directory.join(source_name);
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
                format!("Could not remove source {} after move", source.display())
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
        .with_context(|| format!("Could not read metadata for {}", path.display()))?;

    if metadata.is_dir() {
        let mut plan = OperationPlan {
            total_bytes: 0,
            total_entries: 1,
        };

        for entry in fs::read_dir(path)
            .with_context(|| format!("Could not read directory {}", path.display()))?
        {
            let entry = entry?;
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
        .with_context(|| format!("Could not read metadata for {}", source.display()))?;
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
            .with_context(|| format!("Could not create target folder {}", target.display()))?;

        progress.processed_entries += 1;
        send_progress(tx, kind.clone(), source, plan, progress, started_at);

        for entry in fs::read_dir(source)
            .with_context(|| format!("Could not read directory {}", source.display()))?
        {
            let entry = entry?;
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
            .with_context(|| format!("Could not create target folder {}", parent.display()))?;
    }

    let mut reader = fs::File::open(source)
        .with_context(|| format!("Could not open source file {}", source.display()))?;
    let mut writer = fs::File::create(target)
        .with_context(|| format!("Could not create target file {}", target.display()))?;
    let mut buffer = vec![0_u8; 1024 * 1024];

    loop {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }

        let read = reader
            .read(&mut buffer)
            .with_context(|| format!("Could not read {}", source.display()))?;
        if read == 0 {
            break;
        }

        writer
            .write_all(&buffer[..read])
            .with_context(|| format!("Could not write {}", target.display()))?;
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
        .with_context(|| format!("Could not read metadata for {}", path.display()))?;

    if metadata.is_dir() {
        for entry in fs::read_dir(path)
            .with_context(|| format!("Could not read directory {}", path.display()))?
        {
            let entry = entry?;
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
            .with_context(|| format!("Could not delete folder {}", path.display()))?;
        progress.processed_entries += 1;
        send_progress(tx, kind.clone(), path, plan, progress, started_at);
        return Ok(());
    }

    let file_len = metadata.len();
    fs::remove_file(path).with_context(|| format!("Could not delete file {}", path.display()))?;
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
    trash::delete(path)
        .with_context(|| format!("Could not move {} to the recycle bin", path.display()))?;
    progress.processed_bytes += deleted.total_bytes;
    progress.processed_entries += deleted.total_entries;
    send_progress(tx, kind.clone(), path, plan, progress, started_at);
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
                    format!("Could not remove conflicting target {}", target.display())
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
    sources: Vec<PathBuf>,
    target: Option<PathBuf>,
    total_bytes: u64,
    total_entries: u64,
    started_at: Instant,
) -> OperationSummary {
    OperationSummary {
        kind,
        sources,
        target,
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

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::{atomic::AtomicBool, mpsc, Arc},
        thread,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    use super::{copy_path, CopyProgress, FileOperationKind, OperationEvent, OperationPlan};

    #[test]
    fn cancelled_directory_copy_cleans_partially_created_target() {
        let test_root = unique_test_dir("copy_cancel_cleanup");
        let source_root = test_root.join("source");
        let nested = source_root.join("nested");
        let target_root = test_root.join("target").join("source");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("large.bin"), vec![7_u8; 64 * 1024 * 1024]).unwrap();

        let plan = OperationPlan {
            total_bytes: 64 * 1024 * 1024,
            total_entries: 3,
        };
        let mut progress = CopyProgress {
            processed_bytes: 0,
            processed_entries: 0,
        };
        let (tx, _rx) = mpsc::channel::<OperationEvent>();
        let (_resolution_tx, resolution_rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancel_flag = Arc::clone(&cancelled);
        let target_watch = target_root.clone();

        let watcher = thread::spawn(move || {
            let child_file = target_watch.join("nested").join("large.bin");
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                if child_file.exists() {
                    cancel_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                    return;
                }
                thread::sleep(Duration::from_millis(10));
            }
            panic!("Timed out waiting for partial target file creation");
        });

        copy_path(
            &source_root,
            &target_root,
            &plan,
            &mut progress,
            Instant::now(),
            &tx,
            &FileOperationKind::Copy,
            &cancelled,
            &resolution_rx,
        )
        .unwrap();
        watcher.join().unwrap();

        assert!(!target_root.exists());
        let _ = fs::remove_dir_all(&test_root);
    }

    fn unique_test_dir(label: &str) -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("rust_commander_{label}_{suffix}"))
    }
}
