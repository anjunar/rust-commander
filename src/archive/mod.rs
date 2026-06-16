mod detector;
mod error;
mod libarchive;
mod path;
mod plugin;
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
pub use libarchive::LibArchiveBackend;
pub use path::safe_join_extract_path;
pub use plugin::PluginArchiveBackend;
pub use registry::ArchiveBackendRegistry;
pub use service::{ArchiveService, ArchiveTaskEvent, ArchiveTaskHandle, ArchiveTaskRequest};
pub use unrar::UnrarBackend;
pub use zip_backend::ZipBackend;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ArchiveCapabilities {
    pub list: bool,
    pub extract_single: bool,
    pub extract_multiple: bool,
    pub extract_all: bool,
    pub test: bool,
    pub password: bool,
    pub progress: bool,
    pub cancel: bool,
    pub create_archive: bool,
    pub update_archive: bool,
    pub delete_entry: bool,
    pub rename_entry: bool,
    pub solid_archive: bool,
    pub multi_volume: bool,
}

#[derive(Clone, Debug)]
pub struct ArchiveEntry {
    pub archive_path: String,
    pub display_name: String,
    pub kind: ArchiveEntryKind,
    pub size: u64,
    pub packed_size: Option<u64>,
    pub modified_time: Option<SystemTime>,
    pub crc: Option<String>,
    pub encrypted: bool,
    pub method: Option<String>,
    pub attributes: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArchiveEntryKind {
    File,
    Directory,
    Symlink,
    Unknown,
}

#[derive(Clone, Debug)]
pub enum ArchiveOperation {
    OpenArchive,
    List,
    ExtractEntry {
        entry_path: String,
        target_dir: PathBuf,
    },
    ExtractEntries {
        entry_paths: Vec<String>,
        target_dir: PathBuf,
    },
    ExtractAll {
        target_dir: PathBuf,
    },
    Test,
}

#[derive(Clone, Debug, Default)]
pub struct ArchiveProgress {
    pub operation: Option<ArchiveOperation>,
    pub current_path: Option<String>,
    pub processed_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    pub processed_entries: Option<u64>,
    pub total_entries: Option<u64>,
    pub percent: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct ArchivePasswordRequest {
    pub archive_path: PathBuf,
    pub backend_name: String,
    pub message: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ArchiveSession {
    inner: Arc<ArchiveSessionInner>,
}

#[derive(Debug)]
struct ArchiveSessionInner {
    backend_id: &'static str,
    archive_path: PathBuf,
    detected_format: Option<ArchiveFormat>,
    cached_entries: Vec<ArchiveEntry>,
    capabilities: ArchiveCapabilities,
}

impl ArchiveSession {
    pub fn new(
        backend_id: &'static str,
        archive_path: PathBuf,
        detected_format: Option<ArchiveFormat>,
        cached_entries: Vec<ArchiveEntry>,
        capabilities: ArchiveCapabilities,
    ) -> Self {
        Self {
            inner: Arc::new(ArchiveSessionInner {
                backend_id,
                archive_path,
                detected_format,
                cached_entries,
                capabilities,
            }),
        }
    }

    pub fn archive_path(&self) -> &Path {
        &self.inner.archive_path
    }

    pub fn detected_format(&self) -> Option<ArchiveFormat> {
        self.inner.detected_format
    }

    pub fn cached_entries(&self) -> &[ArchiveEntry] {
        &self.inner.cached_entries
    }

    pub fn capabilities(&self) -> &ArchiveCapabilities {
        &self.inner.capabilities
    }

    pub(crate) fn backend_id(&self) -> &'static str {
        self.inner.backend_id
    }
}

pub trait ArchiveBackend: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn priority(&self) -> u32;
    fn supported_extensions(&self) -> &'static [&'static str];
    fn capabilities(&self) -> ArchiveCapabilities;
    fn can_open(&self, path: &Path) -> bool;
    fn open(&self, path: &Path) -> Result<ArchiveSession, ArchiveError>;
    fn list_entries(&self, session: &ArchiveSession) -> Result<Vec<ArchiveEntry>, ArchiveError>;
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
    fn extract_all(&self, session: &ArchiveSession, target_dir: &Path) -> Result<(), ArchiveError>;
    fn test_archive(&self, session: &ArchiveSession) -> Result<(), ArchiveError>;
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
