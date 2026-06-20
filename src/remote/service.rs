use std::{
    env, fs,
    io::{ErrorKind, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, bail, Context, Result};
use ssh2::{CheckResult, FileStat, HashType, KnownHostFileKind, Session, Sftp};

use crate::{
    application::{
        FileOperationKind, OperationEvent, OperationSnapshot, OperationSummary,
        RemoteDownloadRequest, RemoteUploadRequest,
    },
    domain::{Entry, EntryKind},
};

use super::{RemoteAuthConfig, RemotePath, RemoteProfile, RemoteRuntimeSecret, RemoteSession};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const SSH_SESSION_TIMEOUT: Duration = Duration::from_secs(30);
const SSH_SESSION_TIMEOUT_MS: u32 = 30_000;

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
        session: &RemoteSession,
        current_path: &str,
        show_hidden_files: bool,
    ) -> Result<Vec<Entry>> {
        let connection = self.connect_sftp(session)?;
        let current_path = RemotePath::new(current_path);
        let mut entries = Vec::new();
        if let Some(parent) = current_path.parent() {
            let mut parent_entry = Entry::parent_link();
            parent_entry.remote_path = Some(parent.to_string());
            entries.push(parent_entry);
        }

        let remote_dir = to_remote_fs_path(&current_path);
        let mut listed = connection
            .sftp
            .readdir(remote_dir.as_path())
            .map_err(|error| {
                anyhow!(describe_sftp_path_error(
                    "read remote directory",
                    current_path.as_str(),
                    &error
                ))
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

            let remote_path = current_path.join(name);
            let is_dir = stat_is_directory(&stat);
            let size_bytes = stat.size.unwrap_or(0);
            let modified_at = stat.mtime.map(system_time_from_unix);
            entries.push(Entry {
                name: name.into(),
                archive_path: None,
                remote_path: Some(remote_path.to_string()),
                kind: if is_dir {
                    EntryKind::Directory
                } else {
                    EntryKind::File
                },
                is_dir,
                size_bytes,
                modified_at,
                attributes: remote_attributes(&stat, is_dir),
                is_parent_link: false,
            });
        }

        Ok(entries)
    }

    pub fn start_download(
        &self,
        source: RemoteDownloadRequest,
    ) -> (RemoteOperationHandle, Receiver<OperationEvent>) {
        let (tx, rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let handle = RemoteOperationHandle {
            cancelled: Arc::clone(&cancelled),
        };
        let service = self.clone();

        thread::spawn(move || {
            let result = service.run_download(source, &tx, &cancelled);
            if let Err(error) = result {
                let _ = tx.send(OperationEvent::Failed(error.to_string()));
            }
        });

        (handle, rx)
    }

    pub fn start_upload(
        &self,
        request: RemoteUploadRequest,
    ) -> (RemoteOperationHandle, Receiver<OperationEvent>) {
        let (tx, rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let handle = RemoteOperationHandle {
            cancelled: Arc::clone(&cancelled),
        };
        let service = self.clone();

        thread::spawn(move || {
            let result = service.run_upload(request, &tx, &cancelled);
            if let Err(error) = result {
                let _ = tx.send(OperationEvent::Failed(error.to_string()));
            }
        });

        (handle, rx)
    }

    fn run_download(
        &self,
        source: RemoteDownloadRequest,
        tx: &mpsc::Sender<OperationEvent>,
        cancelled: &Arc<AtomicBool>,
    ) -> Result<()> {
        let target_directory = source.target_directory.clone();
        let connection = self.connect_sftp(&source.session)?;
        let remote_paths = source
            .entry_paths
            .iter()
            .map(RemotePath::new)
            .collect::<Vec<_>>();
        let plan = self.build_remote_plan(&connection.sftp, &remote_paths)?;
        let started_at = Instant::now();
        let mut progress = TransferProgress::default();

        for remote_path in &remote_paths {
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
        request: RemoteUploadRequest,
        tx: &mpsc::Sender<OperationEvent>,
        cancelled: &Arc<AtomicBool>,
    ) -> Result<()> {
        let connection = self.connect_sftp(&request.session)?;
        let plan = build_local_plan(&request.sources)?;
        let started_at = Instant::now();
        let mut progress = TransferProgress::default();
        let remote_target_directory = RemotePath::new(&request.target_directory);

        for source in &request.sources {
            let file_name = source
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or_else(|| anyhow!("Local source has no file name: {}", source.display()))?;
            let remote_target = remote_target_directory.join(file_name);
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
                    request.sources.clone(),
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
            request.sources,
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
            .map_err(|error| {
                anyhow!(describe_sftp_path_error(
                    "read remote path details",
                    remote_path.as_str(),
                    &error
                ))
            })?;
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
            .map_err(|error| {
                anyhow!(describe_sftp_path_error(
                    "open remote file",
                    remote_path.as_str(),
                    &error
                ))
            })?;
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
            remove_partial_local_file(local_target)?;
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
            .map_err(|error| {
                anyhow!(describe_sftp_path_error(
                    "create remote file",
                    remote_target.as_str(),
                    &error
                ))
            })?;
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
            remove_partial_remote_file(sftp, remote_target)?;
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
            .map_err(|error| {
                anyhow!(describe_sftp_path_error(
                    "read remote path details",
                    path.as_str(),
                    &error
                ))
            })?;
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
            .map_err(|error| {
                anyhow!(describe_sftp_path_error(
                    "read remote directory",
                    path.as_str(),
                    &error
                ))
            })?
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
                .map_err(|error| {
                    anyhow!(describe_sftp_path_error(
                        "create remote directory",
                        current.as_str(),
                        &error
                    ))
                })?;
        }
        Ok(())
    }

    fn connect_sftp(&self, remote_session: &RemoteSession) -> Result<ConnectedRemote> {
        let profile = remote_session.profile();
        let tcp = connect_tcp(profile)?;
        let mut session = Session::new().context("Could not create SSH session")?;
        session.set_timeout(SSH_SESSION_TIMEOUT_MS);
        session.set_tcp_stream(tcp);
        session.handshake().map_err(|error| {
            anyhow!(describe_ssh_session_error(
                "complete the SSH handshake",
                &error
            ))
        })?;
        if !profile.skip_host_key_verification {
            verify_known_host(&session, profile)?;
        }

        match (&profile.auth, remote_session.secret()) {
            (RemoteAuthConfig::Password { username }, RemoteRuntimeSecret::Password(password)) => {
                session
                    .userauth_password(username, password)
                    .map_err(|error| {
                        anyhow!(describe_auth_error("password", username, profile, &error))
                    })?;
            }
            (
                RemoteAuthConfig::KeyFile {
                    username,
                    private_key_path,
                    public_key_path,
                },
                RemoteRuntimeSecret::KeyPassphrase(passphrase),
            ) => {
                ensure_key_files_exist(private_key_path, public_key_path.as_deref())?;
                session
                    .userauth_pubkey_file(
                        username,
                        public_key_path.as_deref(),
                        private_key_path,
                        Some(passphrase),
                    )
                    .map_err(|error| {
                        anyhow!(describe_auth_error("public key", username, profile, &error))
                    })?;
            }
            (
                RemoteAuthConfig::KeyFile {
                    username,
                    private_key_path,
                    public_key_path,
                },
                RemoteRuntimeSecret::None,
            ) => {
                ensure_key_files_exist(private_key_path, public_key_path.as_deref())?;
                session
                    .userauth_pubkey_file(
                        username,
                        public_key_path.as_deref(),
                        private_key_path,
                        None,
                    )
                    .map_err(|error| {
                        anyhow!(describe_auth_error("public key", username, profile, &error))
                    })?;
            }
            (RemoteAuthConfig::Password { .. }, _) => {
                bail!("A password is required for this remote profile");
            }
            (RemoteAuthConfig::KeyFile { .. }, _) => {
                bail!("This key-based profile requires a passphrase or no passphrase");
            }
        }

        if !session.authenticated() {
            bail!(
                "Authentication failed for {}@{}:{}",
                profile.auth.username(),
                profile.host,
                profile.port
            );
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

fn connect_tcp(profile: &RemoteProfile) -> Result<TcpStream> {
    let address = format!("{}:{}", profile.host, profile.port);
    let resolved = (profile.host.as_str(), profile.port)
        .to_socket_addrs()
        .with_context(|| format!("Could not resolve remote host {address}"))?
        .collect::<Vec<_>>();
    if resolved.is_empty() {
        bail!("Could not resolve remote host {address}");
    }

    let mut last_error = None;
    for socket_address in resolved {
        match TcpStream::connect_timeout(&socket_address, CONNECT_TIMEOUT) {
            Ok(stream) => {
                stream
                    .set_read_timeout(Some(SSH_SESSION_TIMEOUT))
                    .with_context(|| format!("Could not configure read timeout for {address}"))?;
                stream
                    .set_write_timeout(Some(SSH_SESSION_TIMEOUT))
                    .with_context(|| format!("Could not configure write timeout for {address}"))?;
                return Ok(stream);
            }
            Err(error) => last_error = Some(error),
        }
    }

    let error = last_error
        .unwrap_or_else(|| std::io::Error::other(format!("No reachable address for {address}")));
    bail!("{}", describe_connect_error(&address, &error));
}

fn verify_known_host(session: &Session, profile: &RemoteProfile) -> Result<()> {
    let known_hosts_path = find_known_hosts_path().ok_or_else(|| {
        anyhow!(
            "SSH host key verification is required, but no OpenSSH known_hosts file was found. \
Add {}:{} to ~/.ssh/known_hosts first.",
            profile.host,
            profile.port
        )
    })?;

    let mut known_hosts = session
        .known_hosts()
        .context("Could not initialize SSH known_hosts verification")?;
    known_hosts
        .read_file(&known_hosts_path, KnownHostFileKind::OpenSSH)
        .with_context(|| {
            format!(
                "Could not read SSH known_hosts file {}",
                known_hosts_path.display()
            )
        })?;

    let (host_key, _) = session
        .host_key()
        .ok_or_else(|| anyhow!("SSH server did not provide a host key during handshake"))?;
    let fingerprint = session
        .host_key_hash(HashType::Sha256)
        .map(format_fingerprint)
        .unwrap_or_else(|| "<unavailable>".into());

    match known_hosts.check_port(&profile.host, profile.port, host_key) {
        CheckResult::Match => Ok(()),
        CheckResult::NotFound => bail!(
            "SSH host key for {}:{} was not found in {}. \
Connect once with OpenSSH to record the host key, then retry. Server fingerprint: SHA256:{}",
            profile.host,
            profile.port,
            known_hosts_path.display(),
            fingerprint
        ),
        CheckResult::Mismatch => bail!(
            "SSH host key mismatch for {}:{} against {}. \
This may indicate a server change or a man-in-the-middle risk. Server fingerprint: SHA256:{}",
            profile.host,
            profile.port,
            known_hosts_path.display(),
            fingerprint
        ),
        CheckResult::Failure => bail!(
            "SSH host key verification failed for {}:{} using {}",
            profile.host,
            profile.port,
            known_hosts_path.display()
        ),
    }
}

fn ensure_key_files_exist(private_key_path: &Path, public_key_path: Option<&Path>) -> Result<()> {
    if !private_key_path.is_file() {
        bail!(
            "Private key file was not found: {}",
            private_key_path.display()
        );
    }
    if let Some(public_key_path) = public_key_path {
        if !public_key_path.is_file() {
            bail!(
                "Public key file was not found: {}",
                public_key_path.display()
            );
        }
    }
    Ok(())
}

fn remove_partial_local_file(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| {
            format!(
                "Download was cancelled, but the partial local file could not be removed: {}",
                path.display()
            )
        }),
    }
}

fn remove_partial_remote_file(sftp: &Sftp, path: &RemotePath) -> Result<()> {
    match sftp.unlink(to_remote_fs_path(path).as_path()) {
        Ok(()) => Ok(()),
        Err(error) if is_remote_not_found_error(&error) => Ok(()),
        Err(error) => bail!(
            "Upload was cancelled, but the partial remote file could not be removed: {} ({})",
            path,
            error.message()
        ),
    }
}

fn describe_connect_error(address: &str, error: &std::io::Error) -> String {
    match error.kind() {
        ErrorKind::TimedOut => {
            format!("Could not reach remote host {address}: connection timed out")
        }
        ErrorKind::ConnectionRefused => {
            format!("Could not reach remote host {address}: connection was refused")
        }
        ErrorKind::NotFound | ErrorKind::AddrNotAvailable => {
            format!("Could not reach remote host {address}: host not found")
        }
        _ => format!("Could not connect to remote host {address}: {error}"),
    }
}

fn describe_ssh_session_error(action: &str, error: &ssh2::Error) -> String {
    let detail = error.message();
    if contains_any(detail, &["timed out", "socket timeout"]) {
        format!("SSH session timed out while trying to {action}: {detail}")
    } else {
        format!("Could not {action}: {detail}")
    }
}

fn describe_auth_error(
    method: &str,
    username: &str,
    profile: &RemoteProfile,
    error: &ssh2::Error,
) -> String {
    let detail = error.message();
    if contains_any(
        detail,
        &[
            "authentication failed",
            "username/publickey combination invalid",
        ],
    ) {
        format!(
            "Authentication failed for {}@{}:{} using {} authentication: {}",
            username, profile.host, profile.port, method, detail
        )
    } else if contains_any(detail, &["timed out", "socket timeout"]) {
        format!(
            "Authentication timed out for {}@{}:{} using {} authentication: {}",
            username, profile.host, profile.port, method, detail
        )
    } else {
        format!(
            "Could not authenticate {}@{}:{} using {} authentication: {}",
            username, profile.host, profile.port, method, detail
        )
    }
}

fn describe_sftp_path_error(action: &str, path: &str, error: &ssh2::Error) -> String {
    let detail = error.message();
    if is_remote_not_found_error(error) {
        format!("Remote path not found while trying to {action}: {path} ({detail})")
    } else if contains_any(detail, &["permission denied", "not authorized"]) {
        format!("Permission denied while trying to {action}: {path} ({detail})")
    } else if contains_any(detail, &["file already exists"]) {
        format!("Remote target already exists: {path} ({detail})")
    } else if contains_any(detail, &["not a directory"]) {
        format!("Remote path is not a directory: {path} ({detail})")
    } else if contains_any(detail, &["timed out", "socket timeout"]) {
        format!("SFTP operation timed out while trying to {action}: {path} ({detail})")
    } else {
        format!("Could not {action} {path}: {detail}")
    }
}

fn is_remote_not_found_error(error: &ssh2::Error) -> bool {
    contains_any(error.message(), &["no such file", "no such path"])
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    let value = value.to_ascii_lowercase();
    needles.iter().any(|needle| value.contains(needle))
}

fn find_known_hosts_path() -> Option<PathBuf> {
    let home = env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .or_else(|| env::var_os("USERPROFILE").filter(|value| !value.is_empty()))
        .or_else(|| {
            let drive = env::var_os("HOMEDRIVE")?;
            let path = env::var_os("HOMEPATH")?;
            if drive.is_empty() || path.is_empty() {
                None
            } else {
                let mut combined = PathBuf::from(drive);
                combined.push(path);
                Some(combined.into_os_string())
            }
        })?;

    known_hosts_candidates(Path::new(&home))
        .into_iter()
        .find(|path| path.is_file())
}

fn known_hosts_candidates(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".ssh").join("known_hosts"),
        home.join(".ssh").join("known_hosts2"),
    ]
}

fn format_fingerprint(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{describe_connect_error, format_fingerprint, known_hosts_candidates};

    #[test]
    fn returns_known_hosts_candidates_in_priority_order() {
        let candidates = known_hosts_candidates(Path::new("/tmp/home"));
        assert_eq!(candidates[0], Path::new("/tmp/home/.ssh/known_hosts"));
        assert_eq!(candidates[1], Path::new("/tmp/home/.ssh/known_hosts2"));
    }

    #[test]
    fn formats_connect_timeout_for_user_facing_error() {
        let error = std::io::Error::new(std::io::ErrorKind::TimedOut, "timed out");
        assert_eq!(
            describe_connect_error("example.com:22", &error),
            "Could not reach remote host example.com:22: connection timed out"
        );
    }

    #[test]
    fn formats_sha256_fingerprint_as_hex() {
        assert_eq!(format_fingerprint(&[0x01, 0xab, 0xff]), "01abff");
    }
}
