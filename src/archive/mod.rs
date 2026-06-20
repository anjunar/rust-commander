
mod detector;
mod error;
mod iso_backend;
mod libarchive;
mod path;
mod plugin;
mod probe;
mod registry;
mod service;
mod unrar;
mod zip_backend;

use std::{
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

pub use detector::{ArchiveFormat, ArchiveFormatDetector};
pub use error::ArchiveError;
pub use iso_backend::IsoBackend;
pub use libarchive::LibArchiveBackend;
pub use path::safe_join_extract_path;
pub use plugin::PluginArchiveBackend;
pub use probe::{ArchiveProbe, ArchiveSupport};
pub use registry::ArchiveBackendRegistry;
pub use service::{ArchiveService, ArchiveTaskEvent, ArchiveTaskHandle, ArchiveTaskRequest};
pub use unrar::UnrarBackend;
pub use zip_backend::ZipBackend;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveEntry {
    pub archive_path: String,
    pub display_name: String,
    pub kind: ArchiveEntryKind,
    pub size: u64,
    pub modified_time: Option<SystemTime>,
    pub attributes: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArchiveEntryKind {
    File,
    Directory,
    #[cfg(not(target_os = "windows"))]
    Symlink,
    #[cfg(not(target_os = "windows"))]
    Unknown,
}

#[derive(Clone, Debug)]
pub enum ArchiveOperation {
    ExtractEntries {
        entry_paths: Vec<String>,
    },
}

#[derive(Clone, Debug, Default)]
pub struct ArchiveProgress {
    pub operation: Option<ArchiveOperation>,
    pub current_path: Option<String>,
    pub processed_entries: Option<u64>,
    pub total_entries: Option<u64>,
    pub percent: Option<f64>,
}


#[derive(Clone, Debug)]
pub struct ArchiveSession {
    inner: Arc<ArchiveSessionInner>,
}

#[derive(Debug)]
struct ArchiveSessionInner {
    backend_id: &'static str,
    archive_path: PathBuf,
    cached_entries: Vec<ArchiveEntry>,
}

impl ArchiveSession {
    pub fn new(
        backend_id: &'static str,
        archive_path: PathBuf,
        cached_entries: Vec<ArchiveEntry>,
    ) -> Self {
        Self {
            inner: Arc::new(ArchiveSessionInner {
                backend_id,
                archive_path,
                cached_entries,
            }),
        }
    }

    pub fn archive_path(&self) -> &Path {
        &self.inner.archive_path
    }

    pub fn cached_entries(&self) -> &[ArchiveEntry] {
        &self.inner.cached_entries
    }

    pub(crate) fn backend_id(&self) -> &'static str {
        self.inner.backend_id
    }
}

pub trait ArchiveBackend: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn priority(&self) -> u32;
    fn can_open(&self, path: &Path) -> bool;
    fn open(&self, path: &Path) -> Result<ArchiveSession, ArchiveError>;
    fn extract_entry(
        &self,
        session: &ArchiveSession,
        entry_path: &str,
        target_dir: &Path,
    ) -> Result<(), ArchiveError>;
    fn extract_entries(
        &self,
        session: &ArchiveSession,
        entry_paths: &[String],
        target_dir: &Path,
    ) -> Result<(), ArchiveError> {
        for entry_path in entry_paths {
            self.extract_entry(session, entry_path, target_dir)?;
        }
        Ok(())
    }
}

impl fmt::Debug for dyn ArchiveBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ArchiveBackend")
            .field("id", &self.id())
            .field("name", &self.name())
            .field("priority", &self.priority())
            .finish()
    }
}
