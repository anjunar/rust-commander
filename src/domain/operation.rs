use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use crate::archive::ArchiveSession;

#[derive(Clone, Debug)]
pub enum FileOperationKind {
    Copy,
    Move,
    Delete,
}

#[derive(Clone, Debug)]
pub struct FileOperationRequest {
    pub kind: FileOperationKind,
    pub sources: Vec<PathBuf>,
    pub target_directory: Option<PathBuf>,
    pub archive_source: Option<ArchiveSourceRequest>,
}

#[derive(Clone, Debug)]
pub struct ArchiveSourceRequest {
    pub session: ArchiveSession,
    pub entry_paths: Vec<String>,
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
    pub sources: Vec<PathBuf>,
    pub target: Option<PathBuf>,
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
    Failed(String),
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
