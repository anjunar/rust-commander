use std::{
    fs,
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
    thread,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, bail, Context, Result};
use ssh2::{FileStat, Session, Sftp};

use crate::{
    domain::{
        operation::{
            FileOperationKind, OperationEvent, OperationSnapshot, OperationSummary,
            RemoteSourceRequest, RemoteTargetRequest,
        },
        Entry,
    },
    fs::reader::format_bytes,
    presentation,
};

use super::{RemoteAuthConfig, RemoteLocation, RemotePath, RemoteRuntimeSecret, RemoteSession};

#[derive(Clone)]
pub struct RemoteOperationHandle {
    cancelled: Arc<AtomicBool>,
}

impl RemoteOperationHandle {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

#[derive(Clone, Default, Debug)]
struct TransferPlan {
    total_bytes: u64,
    total_entries: u64,
}

#[derive(Clone, Default, Debug)]
struct TransferProgress {
    processed_bytes: u64,
    processed_entries: u64,
}

#[derive(Clone, Debug, Default)]
pub struct RemoteService;

struct ConnectedRemote {
    _session: Session,
    sftp: Sftp,
}

impl RemoteService {
    pub fn read_entries(
        &self,
        location: &RemoteLocation,
        show_hidden_files: bool,
    ) -> Result<Vec<Entry>> {
        let connection = self.connect_sftp(&location.session)?;
        let mut entries = Vec::new();
        if let Some(parent) = location.current_path.parent() {
            let mut parent_entry = Entry::parent_link(presentation::parent_entry_type_label());
            parent_entry.remote_path = Some(parent);
            entries.push(parent_entry);
        }

        let remote_dir = to_remote_fs_path(&location.current_path);
        let mut listed = connection
            .sftp
            .readdir(remote_dir.as_path())
            .with_context(|| {
                format!("Could not read remote directory {}", location.current_path)
            })?;
        listed.sort_by(|(left, _), (right, _)| left.cmp(right));

        for (path, stat) in listed.drain(..) {
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if matches!(name, "." | "..") {
                continue;
            }
            if !show_hidden_files && name.starts_with('.') {
                continue;
            }

            let remote_path = location.current_path.join(name);
            let is_dir = stat_is_directory(&stat);
            let size_bytes = stat.size.unwrap_or(0);
            let modified_at = stat.mtime.map(system_time_from_unix);
            entries.push(Entry {
                name: name.into(),
                archive_path: None,
                remote_path: Some(remote_path),
                is_dir,
                size_bytes,
                size_label: if is_dir {
                    "-".into()
                } else {
                    format_bytes(size_bytes)
                },
                type_label: presentation::filesystem_entry_type_label(is_dir),
                modified_at,
                modified_label: modified_at
                    .map(crate::fs::reader::format_system_time)
                    .unwrap_or_default(),
                attributes_label: remote_attributes(&stat, is_dir),
                is_parent_link: false,
            });
        }

        Ok(entries)
    }

    pub fn start_download(
        &self,
        source: RemoteSourceRequest,
        target_directory: PathBuf,
    ) -> (RemoteOperationHandle, Receiver<OperationEvent>) {
        let (tx, rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let handle = RemoteOperationHandle {
            cancelled: Arc::clone(&cancelled),
        };
        let service = self.clone();

        thread::spawn(move || {
            let result = service.run_download(source, target_directory, &tx, &cancelled);
            if let Err(error) = result {
                let _ = tx.send(OperationEvent::Failed(error.to_string()));
            }
        });

        (handle, rx)
    }

    pub fn start_upload(
        &self,
        sources: Vec<PathBuf>,
        target: RemoteTargetRequest,
    ) -> (RemoteOperationHandle, Receiver<OperationEvent>) {
        let (tx, rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let handle = RemoteOperationHandle {
            cancelled: Arc::clone(&cancelled),
        };
        let service = self.clone();

        thread::spawn(move || {
            let result = service.run_upload(sources, target, &tx, &cancelled);
            if let Err(error) = result {
                let _ = tx.send(OperationEvent::Failed(error.to_string()));
            }
        });

        (handle, rx)
    }

    fn run_download(
        &self,
        source: RemoteSourceRequest,
        target_directory: PathBuf,
        tx: &mpsc::Sender<OperationEvent>,
        cancelled: &Arc<AtomicBool>,
    ) -> Result<()> {
        let connection = self.connect_sftp(&source.session)?;
        let plan = self.build_remote_plan(&connection.sftp, &source.entry_paths)?;
        let started_at = Instant::now();
        let mut progress = TransferProgress::default();

        for remote_path in &source.entry_paths {
            let file_name = remote_path
                .file_name()
                .ok_or_else(|| anyhow!("Remote source has no file name: {remote_path}"))?;
            let local_target = target_directory.join(file_name);
            self.download_path(
                &connection.sftp,
                remote_path,
                &local_target,
                &plan,
                &mut progress,
                started_at,
                tx,
                cancelled,
            )?;
            if cancelled.load(Ordering::Relaxed) {
                let _ = tx.send(OperationEvent::Cancelled(operation_summary(
                    FileOperationKind::Copy,
                    Vec::new(),
                    Some(target_directory.clone()),
                    progress.processed_bytes,
                    progress.processed_entries,
                    started_at,
                )));
                return Ok(());
            }
        }

        let _ = tx.send(OperationEvent::Finished(operation_summary(
            FileOperationKind::Copy,
            Vec::new(),
            Some(target_directory),
            plan.total_bytes,
            plan.total_entries,
            started_at,
        )));
        Ok(())
    }

    fn run_upload(
        &self,
        sources: Vec<PathBuf>,
        target: RemoteTargetRequest,
        tx: &mpsc::Sender<OperationEvent>,
        cancelled: &Arc<AtomicBool>,
    ) -> Result<()> {
        let connection = self.connect_sftp(&target.session)?;
        let plan = build_local_plan(&sources)?;
        let started_at = Instant::now();
        let mut progress = TransferProgress::default();

        for source in &sources {
            let file_name = source
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or_else(|| anyhow!("Local source has no file name: {}", source.display()))?;
            let remote_target = target.target_directory.join(file_name);
            self.upload_path(
                &connection.sftp,
                source,
                &remote_target,
                &plan,
                &mut progress,
                started_at,
                tx,
                cancelled,
            )?;
            if cancelled.load(Ordering::Relaxed) {
                let _ = tx.send(OperationEvent::Cancelled(operation_summary(
                    FileOperationKind::Copy,
                    sources.clone(),
                    None,
                    progress.processed_bytes,
                    progress.processed_entries,
                    started_at,
                )));
                return Ok(());
            }
        }

        let _ = tx.send(OperationEvent::Finished(operation_summary(
            FileOperationKind::Copy,
            sources,
            None,
            plan.total_bytes,
            plan.total_entries,
            started_at,
        )));
        Ok(())
    }

    fn download_path(
        &self,
        sftp: &Sftp,
        remote_path: &RemotePath,
        local_target: &Path,
        plan: &TransferPlan,
        progress: &mut TransferProgress,
        started_at: Instant,
        tx: &mpsc::Sender<OperationEvent>,
        cancelled: &Arc<AtomicBool>,
    ) -> Result<()> {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(());
        }

        let stat = sftp
            .stat(to_remote_fs_path(remote_path).as_path())
            .with_context(|| format!("Could not stat remote path {remote_path}"))?;
        if stat_is_directory(&stat) {
            fs::create_dir_all(local_target)
                .with_context(|| format!("Could not create {}", local_target.display()))?;
            progress.processed_entries += 1;
            send_progress(
                tx,
                FileOperationKind::Copy,
                remote_path.to_string(),
                plan,
                progress,
                started_at,
            );

            for child in self.read_child_paths(sftp, remote_path)? {
                let file_name = child
                    .file_name()
                    .ok_or_else(|| anyhow!("Remote child path has no file name: {child}"))?;
                self.download_path(
                    sftp,
                    &child,
                    &local_target.join(file_name),
                    plan,
                    progress,
                    started_at,
                    tx,
                    cancelled,
                )?;
            }
            return Ok(());
        }

        if let Some(parent) = local_target.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Could not create {}", parent.display()))?;
        }
        if local_target.exists() {
            bail!("Target already exists: {}", local_target.display());
        }

        let mut remote_file = sftp
            .open(to_remote_fs_path(remote_path).as_path())
            .with_context(|| format!("Could not open remote file {remote_path}"))?;
        let mut local_file = fs::File::create(local_target)
            .with_context(|| format!("Could not create {}", local_target.display()))?;
        let mut buffer = vec![0_u8; 1024 * 1024];

        loop {
            if cancelled.load(Ordering::Relaxed) {
                break;
            }
            let read = remote_file
                .read(&mut buffer)
                .with_context(|| format!("Could not read remote file {remote_path}"))?;
            if read == 0 {
                break;
            }
            local_file
                .write_all(&buffer[..read])
                .with_context(|| format!("Could not write {}", local_target.display()))?;
            progress.processed_bytes += read as u64;
            send_progress(
                tx,
                FileOperationKind::Copy,
                remote_path.to_string(),
                plan,
                progress,
                started_at,
            );
        }

        if cancelled.load(Ordering::Relaxed) {
            let _ = fs::remove_file(local_target);
            return Ok(());
        }

        progress.processed_entries += 1;
        send_progress(
            tx,
            FileOperationKind::Copy,
            remote_path.to_string(),
            plan,
            progress,
            started_at,
        );
        Ok(())
    }

    fn upload_path(
        &self,
        sftp: &Sftp,
        source: &Path,
        remote_target: &RemotePath,
        plan: &TransferPlan,
        progress: &mut TransferProgress,
        started_at: Instant,
        tx: &mpsc::Sender<OperationEvent>,
        cancelled: &Arc<AtomicBool>,
    ) -> Result<()> {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(());
        }

        let metadata = fs::metadata(source)
            .with_context(|| format!("Could not read metadata for {}", source.display()))?;
        if metadata.is_dir() {
            self.create_remote_dir_all(sftp, remote_target)?;
            progress.processed_entries += 1;
            send_progress(
                tx,
                FileOperationKind::Copy,
                source.display().to_string(),
                plan,
                progress,
                started_at,
            );

            for entry in fs::read_dir(source)
                .with_context(|| format!("Could not read directory {}", source.display()))?
            {
                let entry = entry?;
                let child_source = entry.path();
                let child_name = entry.file_name().to_string_lossy().into_owned();
                self.upload_path(
                    sftp,
                    &child_source,
                    &remote_target.join(child_name),
                    plan,
                    progress,
                    started_at,
                    tx,
                    cancelled,
                )?;
            }
            return Ok(());
        }

        if sftp
            .stat(to_remote_fs_path(remote_target).as_path())
            .is_ok()
        {
            bail!("Remote target already exists: {remote_target}");
        }

        if let Some(parent) = remote_target.parent() {
            self.create_remote_dir_all(sftp, &parent)?;
        }

        let mut local_file = fs::File::open(source)
            .with_context(|| format!("Could not open {}", source.display()))?;
        let mut remote_file = sftp
            .create(to_remote_fs_path(remote_target).as_path())
            .with_context(|| format!("Could not create remote file {remote_target}"))?;
        let mut buffer = vec![0_u8; 1024 * 1024];

        loop {
            if cancelled.load(Ordering::Relaxed) {
                break;
            }
            let read = local_file
                .read(&mut buffer)
                .with_context(|| format!("Could not read {}", source.display()))?;
            if read == 0 {
                break;
            }
            remote_file
                .write_all(&buffer[..read])
                .with_context(|| format!("Could not write remote file {remote_target}"))?;
            progress.processed_bytes += read as u64;
            send_progress(
                tx,
                FileOperationKind::Copy,
                source.display().to_string(),
                plan,
                progress,
                started_at,
            );
        }

        if cancelled.load(Ordering::Relaxed) {
            let _ = sftp.unlink(to_remote_fs_path(remote_target).as_path());
            return Ok(());
        }

        progress.processed_entries += 1;
        send_progress(
            tx,
            FileOperationKind::Copy,
            source.display().to_string(),
            plan,
            progress,
            started_at,
        );
        Ok(())
    }

    fn build_remote_plan(&self, sftp: &Sftp, paths: &[RemotePath]) -> Result<TransferPlan> {
        let mut plan = TransferPlan::default();
        for path in paths {
            let child = self.build_remote_plan_for_path(sftp, path)?;
            plan.total_bytes += child.total_bytes;
            plan.total_entries += child.total_entries;
        }
        Ok(plan)
    }

    fn build_remote_plan_for_path(&self, sftp: &Sftp, path: &RemotePath) -> Result<TransferPlan> {
        let stat = sftp
            .stat(to_remote_fs_path(path).as_path())
            .with_context(|| format!("Could not stat remote path {path}"))?;
        if stat_is_directory(&stat) {
            let mut plan = TransferPlan {
                total_bytes: 0,
                total_entries: 1,
            };
            for child in self.read_child_paths(sftp, path)? {
                let child_plan = self.build_remote_plan_for_path(sftp, &child)?;
                plan.total_bytes += child_plan.total_bytes;
                plan.total_entries += child_plan.total_entries;
            }
            Ok(plan)
        } else {
            Ok(TransferPlan {
                total_bytes: stat.size.unwrap_or(0),
                total_entries: 1,
            })
        }
    }

    fn read_child_paths(&self, sftp: &Sftp, path: &RemotePath) -> Result<Vec<RemotePath>> {
        let mut children = Vec::new();
        for (child_path, _) in sftp
            .readdir(to_remote_fs_path(path).as_path())
            .with_context(|| format!("Could not read remote directory {path}"))?
        {
            let Some(name) = child_path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if matches!(name, "." | "..") {
                continue;
            }
            children.push(path.join(name));
        }
        Ok(children)
    }

    fn create_remote_dir_all(&self, sftp: &Sftp, path: &RemotePath) -> Result<()> {
        if path.is_root() {
            return Ok(());
        }
        let mut current = RemotePath::root();
        for part in path.as_str().split('/').filter(|part| !part.is_empty()) {
            current = current.join(part);
            let remote_fs_path = to_remote_fs_path(&current);
            if sftp.stat(remote_fs_path.as_path()).is_ok() {
                continue;
            }
            sftp.mkdir(remote_fs_path.as_path(), 0o755)
                .with_context(|| format!("Could not create remote directory {current}"))?;
        }
        Ok(())
    }

    fn connect_sftp(&self, remote_session: &RemoteSession) -> Result<ConnectedRemote> {
        let address = format!(
            "{}:{}",
            remote_session.profile().host,
            remote_session.profile().port
        );
        let tcp = TcpStream::connect(&address)
            .with_context(|| format!("Could not connect to {address}"))?;
        let mut session = Session::new().context("Could not create SSH session")?;
        session.set_tcp_stream(tcp);
        session.handshake().context("SSH handshake failed")?;

        match (&remote_session.profile().auth, remote_session.secret()) {
            (RemoteAuthConfig::Password { username }, RemoteRuntimeSecret::Password(password)) => {
                session
                    .userauth_password(username, password)
                    .with_context(|| format!("Password authentication failed for {username}"))?;
            }
            (
                RemoteAuthConfig::KeyFile {
                    username,
                    private_key_path,
                    public_key_path,
                },
                RemoteRuntimeSecret::KeyPassphrase(passphrase),
            ) => {
                session
                    .userauth_pubkey_file(
                        username,
                        public_key_path.as_deref(),
                        private_key_path,
                        Some(passphrase),
                    )
                    .with_context(|| format!("Key authentication failed for {username}"))?;
            }
            (
                RemoteAuthConfig::KeyFile {
                    username,
                    private_key_path,
                    public_key_path,
                },
                RemoteRuntimeSecret::None,
            ) => {
                session
                    .userauth_pubkey_file(
                        username,
                        public_key_path.as_deref(),
                        private_key_path,
                        None,
                    )
                    .with_context(|| format!("Key authentication failed for {username}"))?;
            }
            (RemoteAuthConfig::Password { .. }, _) => {
                bail!("A password is required for this remote profile");
            }
            (RemoteAuthConfig::KeyFile { .. }, _) => {
                bail!("This key-based profile requires a passphrase or no passphrase");
            }
        }

        if !session.authenticated() {
            bail!("SSH authentication failed");
        }

        let sftp = session.sftp().context("Could not start SFTP session")?;
        Ok(ConnectedRemote {
            _session: session,
            sftp,
        })
    }
}

fn build_local_plan(paths: &[PathBuf]) -> Result<TransferPlan> {
    let mut total = TransferPlan::default();
    for path in paths {
        let metadata =
            fs::metadata(path).with_context(|| format!("Could not read {}", path.display()))?;
        if metadata.is_dir() {
            total.total_entries += 1;
            for entry in fs::read_dir(path)
                .with_context(|| format!("Could not read directory {}", path.display()))?
            {
                let child = build_local_plan(&[entry?.path()])?;
                total.total_bytes += child.total_bytes;
                total.total_entries += child.total_entries;
            }
        } else {
            total.total_bytes += metadata.len();
            total.total_entries += 1;
        }
    }
    Ok(total)
}

fn remote_attributes(stat: &FileStat, is_dir: bool) -> String {
    match stat.perm {
        Some(perm) => format!("{perm:o}"),
        None if is_dir => "DIR".into(),
        None => String::new(),
    }
}

fn stat_is_directory(stat: &FileStat) -> bool {
    const S_IFMT: u32 = 0o170000;
    const S_IFDIR: u32 = 0o040000;
    stat.perm
        .map(|perm| perm & S_IFMT == S_IFDIR)
        .unwrap_or(false)
}

fn send_progress(
    tx: &mpsc::Sender<OperationEvent>,
    kind: FileOperationKind,
    current_item: String,
    plan: &TransferPlan,
    progress: &TransferProgress,
    started_at: Instant,
) {
    let _ = tx.send(OperationEvent::Progress(OperationSnapshot {
        kind,
        current_item,
        processed_bytes: progress.processed_bytes,
        total_bytes: plan.total_bytes,
        processed_entries: progress.processed_entries,
        total_entries: plan.total_entries,
        started_at,
    }));
}

fn operation_summary(
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

fn to_remote_fs_path(path: &RemotePath) -> PathBuf {
    PathBuf::from(path.as_str())
}

fn system_time_from_unix(value: u64) -> SystemTime {
    UNIX_EPOCH + std::time::Duration::from_secs(value)
}
