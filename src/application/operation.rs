use crate::application::OperationError;

use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use crate::{archive::ArchiveSession, remote::RemoteSession};

#[derive(Clone, Debug)]
pub enum FileOperationKind {
    Copy,
    Move,
    Delete,
}

#[derive(Clone, Debug)]
pub struct LocalOperationRequest {
    pub kind: FileOperationKind,
    pub sources: Vec<PathBuf>,
    pub target_directory: Option<PathBuf>,
    pub use_recycle_bin: bool,
}

#[derive(Clone, Debug)]
pub struct ArchiveExtractRequest {
    pub session: ArchiveSession,
    pub entry_paths: Vec<String>,
    pub target_directory: PathBuf,
}

#[derive(Clone, Debug)]
pub struct RemoteDownloadRequest {
    pub session: RemoteSession,
    pub entry_paths: Vec<String>,
    pub target_directory: PathBuf,
}

#[derive(Clone, Debug)]
pub struct RemoteUploadRequest {
    pub sources: Vec<PathBuf>,
    pub session: RemoteSession,
    pub target_directory: String,
}

#[derive(Clone, Debug)]
pub enum OperationPlan {
    Local(LocalOperationRequest),
    ArchiveExtract(ArchiveExtractRequest),
    RemoteDownload(RemoteDownloadRequest),
    RemoteUpload(RemoteUploadRequest),
}

impl OperationPlan {
    pub fn kind(&self) -> FileOperationKind {
        match self {
            Self::Local(request) => request.kind.clone(),
            Self::ArchiveExtract(_) | Self::RemoteDownload(_) | Self::RemoteUpload(_) => {
                FileOperationKind::Copy
            }
        }
    }

    pub fn with_use_recycle_bin(mut self, use_recycle_bin: bool) -> Self {
        if let Self::Local(request) = &mut self {
            request.use_recycle_bin = use_recycle_bin;
        }
        self
    }

    pub fn local_sources(&self) -> &[PathBuf] {
        match self {
            Self::Local(request) => &request.sources,
            Self::RemoteUpload(request) => &request.sources,
            Self::ArchiveExtract(_) | Self::RemoteDownload(_) => &[],
        }
    }
}

#[derive(Clone, Debug)]
pub struct OperationSnapshot {
    pub kind: FileOperationKind,
    pub current_item: String,
    pub processed_bytes: u64,
    pub total_bytes: u64,
    pub processed_entries: u64,
    pub total_entries: u64,
    pub started_at: Instant,
}

#[derive(Clone, Debug)]
pub struct OperationSummary {
    pub kind: FileOperationKind,
    pub total_bytes: u64,
    pub total_entries: u64,
    pub elapsed: Duration,
}

#[derive(Clone, Debug)]
pub enum OperationEvent {
    Progress(OperationSnapshot),
    Conflict(OperationConflict),
    Finished(OperationSummary),
    Cancelled(OperationSummary),
    Failed(OperationError),
}

#[derive(Clone, Debug)]
pub struct OperationConflict {
    pub kind: FileOperationKind,
    pub source: PathBuf,
    pub target: PathBuf,
}

#[derive(Clone, Debug)]
pub enum ConflictResolution {
    Overwrite,
    Skip,
    Rename,
    Cancel,
}
