use std::{cmp::Reverse, path::Path, sync::Arc};

use super::{
    ArchiveBackend, ArchiveError, ArchiveFormatDetector, LibArchiveBackend, PluginArchiveBackend,
    UnrarBackend, ZipBackend,
};

#[derive(Clone, Debug, Default)]
pub struct ArchiveBackendRegistry {
    backends: Arc<Vec<Arc<dyn ArchiveBackend>>>,
}

impl ArchiveBackendRegistry {
    pub fn new(mut backends: Vec<Arc<dyn ArchiveBackend>>) -> Self {
        backends.sort_by_key(|backend| Reverse(backend.priority()));
        Self {
            backends: Arc::new(backends),
        }
    }

    pub fn with_default_backends() -> Self {
        Self::new(vec![
            Arc::new(ZipBackend::new()),
            Arc::new(LibArchiveBackend::new_stub()),
            Arc::new(UnrarBackend::new()),
            Arc::new(PluginArchiveBackend::new_stub()),
        ])
    }

    pub fn is_archive_path(&self, path: &Path) -> bool {
        ArchiveFormatDetector::is_supported_archive(path)
    }

    pub fn resolve_for_path(&self, path: &Path) -> Result<Arc<dyn ArchiveBackend>, ArchiveError> {
        self.backends
            .iter()
            .find(|backend| backend.can_open(path))
            .cloned()
            .ok_or_else(|| {
                if self.is_archive_path(path) {
                    ArchiveError::BackendNotFound {
                        backend: "registered archive backend".into(),
                        path: Some(path.to_path_buf()),
                    }
                } else {
                    ArchiveError::UnsupportedFormat {
                        path: path.to_path_buf(),
                    }
                }
            })
    }

    pub fn backend_for_session(
        &self,
        session: &super::ArchiveSession,
    ) -> Result<Arc<dyn ArchiveBackend>, ArchiveError> {
        self.backends
            .iter()
            .find(|backend| backend.id() == session.backend_id())
            .cloned()
            .ok_or_else(|| ArchiveError::BackendNotFound {
                backend: session.backend_id().into(),
                path: Some(session.archive_path().to_path_buf()),
            })
    }
}
