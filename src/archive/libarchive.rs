use std::path::Path;

use super::{
    ArchiveBackend, ArchiveCapabilities, ArchiveEntry, ArchiveError, ArchiveFormat,
    ArchiveFormatDetector, ArchiveSession,
};

#[derive(Clone, Debug, Default)]
pub struct LibArchiveBackend;

impl LibArchiveBackend {
    pub fn new_stub() -> Self {
        Self
    }

    fn unsupported(path: &Path) -> ArchiveError {
        ArchiveError::FeatureNotSupported {
            backend: "libarchive".into(),
            feature: format!("Opening {} via native libarchive backend", path.display()),
        }
    }
}

impl ArchiveBackend for LibArchiveBackend {
    fn id(&self) -> &'static str {
        "libarchive"
    }

    fn name(&self) -> &'static str {
        "libarchive backend"
    }

    fn priority(&self) -> u32 {
        200
    }

    fn supported_extensions(&self) -> &'static [&'static str] {
        &[
            "tar", "tar.gz", "tgz", "tar.bz2", "tbz2", "tar.xz", "txz", "gz", "bz2", "xz",
            "cab", "iso", "wim", "cpio",
        ]
    }

    fn capabilities(&self) -> ArchiveCapabilities {
        ArchiveCapabilities {
            list: true,
            extract_single: true,
            extract_multiple: true,
            extract_all: true,
            test: true,
            ..ArchiveCapabilities::default()
        }
    }

    fn can_open(&self, _path: &Path) -> bool {
        false
    }

    fn open(&self, path: &Path) -> Result<ArchiveSession, ArchiveError> {
        let detected = ArchiveFormatDetector::detect(path)
            .or(Some(ArchiveFormat::Tar));
        Err(match detected {
            Some(_) => Self::unsupported(path),
            None => ArchiveError::UnsupportedFormat {
                path: path.to_path_buf(),
            },
        })
    }

    fn list_entries(&self, session: &ArchiveSession) -> Result<Vec<ArchiveEntry>, ArchiveError> {
        Err(Self::unsupported(session.archive_path()))
    }

    fn extract_entry(
        &self,
        session: &ArchiveSession,
        _entry_path: &str,
        _target_dir: &Path,
    ) -> Result<(), ArchiveError> {
        Err(Self::unsupported(session.archive_path()))
    }

    fn extract_entries(
        &self,
        session: &ArchiveSession,
        _entry_paths: &[String],
        _target_dir: &Path,
    ) -> Result<(), ArchiveError> {
        Err(Self::unsupported(session.archive_path()))
    }

    fn extract_all(
        &self,
        session: &ArchiveSession,
        _target_dir: &Path,
    ) -> Result<(), ArchiveError> {
        Err(Self::unsupported(session.archive_path()))
    }

    fn test_archive(&self, session: &ArchiveSession) -> Result<(), ArchiveError> {
        Err(Self::unsupported(session.archive_path()))
    }
}
