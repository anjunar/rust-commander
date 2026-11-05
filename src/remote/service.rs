use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, Sender},
        Arc, Mutex,
    },
    time::Instant,
};

use anyhow::Result;
use ssh2::{HostKeyType, KnownHostKeyFormat, Session, Sftp};

use crate::application::{ConflictResolution, TaskSpawner};

#[path = "service_connect.rs"]
mod connect;
#[path = "service_listing.rs"]
mod listing;
#[path = "service_transfer.rs"]
mod transfer;
#[path = "service_util.rs"]
mod util;

pub(super) const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
pub(super) const SSH_SESSION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
pub(super) const SSH_SESSION_TIMEOUT_MS: u32 = 30_000;

/// Host key presented by the server when it is not yet trusted in `known_hosts`.
#[derive(Clone, Debug)]
pub struct HostKeyTrustOffer {
    pub host: String,
    pub port: u16,
    pub fingerprint_sha256: String,
    pub host_key: Vec<u8>,
    pub key_type: HostKeyAlgorithm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostKeyAlgorithm {
    Rsa,
    Dss,
    Ecdsa256,
    Ecdsa384,
    Ecdsa521,
    Ed25519,
    Unknown,
}

impl HostKeyAlgorithm {
    fn from_ssh2(key_type: HostKeyType) -> Self {
        match key_type {
            HostKeyType::Rsa => Self::Rsa,
            HostKeyType::Dss => Self::Dss,
            HostKeyType::Ecdsa256 => Self::Ecdsa256,
            HostKeyType::Ecdsa384 => Self::Ecdsa384,
            HostKeyType::Ecdsa521 => Self::Ecdsa521,
            HostKeyType::Ed25519 => Self::Ed25519,
            HostKeyType::Unknown => Self::Unknown,
        }
    }

    fn to_known_host_format(self) -> KnownHostKeyFormat {
        match self {
            Self::Rsa => KnownHostKeyFormat::SshRsa,
            Self::Dss => KnownHostKeyFormat::SshDss,
            Self::Ecdsa256 => KnownHostKeyFormat::Ecdsa256,
            Self::Ecdsa384 => KnownHostKeyFormat::Ecdsa384,
            Self::Ecdsa521 => KnownHostKeyFormat::Ecdsa521,
            Self::Ed25519 => KnownHostKeyFormat::Ed25519,
            Self::Unknown => KnownHostKeyFormat::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Rsa => "ssh-rsa",
            Self::Dss => "ssh-dss",
            Self::Ecdsa256 => "ecdsa-sha2-nistp256",
            Self::Ecdsa384 => "ecdsa-sha2-nistp384",
            Self::Ecdsa521 => "ecdsa-sha2-nistp521",
            Self::Ed25519 => "ssh-ed25519",
            Self::Unknown => "unknown",
        }
    }
}

/// Returned (via `anyhow`) when the server host key is not yet trusted.
#[derive(Clone, Debug)]
pub struct HostKeyUntrusted(pub HostKeyTrustOffer);

impl std::fmt::Display for HostKeyUntrusted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SSH host key for {}:{} is not trusted yet. Server fingerprint: SHA256:{}",
            self.0.host, self.0.port, self.0.fingerprint_sha256
        )
    }
}

impl std::error::Error for HostKeyUntrusted {}

#[derive(Clone)]
pub struct RemoteOperationHandle {
    cancelled: Arc<AtomicBool>,
    resolution_tx: Sender<ConflictResolution>,
}

impl RemoteOperationHandle {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn resolve_conflict(&self, resolution: ConflictResolution) {
        let _ = self.resolution_tx.send(resolution);
    }
}

#[derive(Clone, Default, Debug)]
pub(super) struct TransferPlan {
    total_bytes: u64,
    total_entries: u64,
}

#[derive(Clone, Default, Debug)]
pub(super) struct TransferProgress {
    processed_bytes: u64,
    processed_entries: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub(super) struct RemoteListingCacheKey {
    session_key: String,
    path: String,
    show_hidden_files: bool,
}

#[derive(Clone, Debug)]
pub struct RemoteService {
    pub(super) task_spawner: TaskSpawner,
    /// Cached SFTP directory listings keyed by session + path + hidden-files flag.
    pub(super) listing_cache: Arc<Mutex<HashMap<RemoteListingCacheKey, Vec<crate::domain::Entry>>>>,
}

pub(super) struct ConnectedRemote {
    pub(super) _session: Session,
    pub(super) sftp: Sftp,
}

pub(super) struct TransferRuntime<'a> {
    pub(super) plan: &'a TransferPlan,
    pub(super) progress: &'a mut TransferProgress,
    pub(super) started_at: Instant,
    pub(super) tx: &'a std::sync::mpsc::Sender<crate::application::OperationEvent>,
    pub(super) cancelled: &'a Arc<AtomicBool>,
    pub(super) resolution_rx: &'a Receiver<ConflictResolution>,
    pub(super) force_overwrite: bool,
}

impl RemoteService {
    pub fn new(task_spawner: TaskSpawner) -> Self {
        Self {
            task_spawner,
            listing_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Persist a previously offered host key into the user's OpenSSH `known_hosts`.
    pub fn trust_host_key(offer: &HostKeyTrustOffer) -> Result<()> {
        connect::trust_host_key(offer)
    }
}

impl Default for RemoteService {
    fn default() -> Self {
        Self::new(TaskSpawner::default())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/remote_service_tests.rs"]
mod tests;
