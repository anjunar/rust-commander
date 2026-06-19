use std::{cmp::Reverse, path::Path, sync::Arc};

use super::{
    probe::{archive_family_for_format, probe_multipart_zip},
    ArchiveBackend, ArchiveError, ArchiveFormatDetector, ArchiveProbe, ArchiveSupport, IsoBackend,
    LibArchiveBackend, PluginArchiveBackend, UnrarBackend, ZipBackend,
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
            Arc::new(IsoBackend::new()),
            Arc::new(LibArchiveBackend::new()),
            Arc::new(UnrarBackend::new()),
            Arc::new(PluginArchiveBackend::new_stub()),
        ])
    }

    pub fn is_archive_path(&self, path: &Path) -> bool {
        self.probe_path(path).is_some()
    }

    pub fn probe_path(&self, path: &Path) -> Option<ArchiveProbe> {
        if let Some(probe) = probe_multipart_zip(path) {
            return Some(probe);
        }

        let detected_format = ArchiveFormatDetector::detect(path)?;
        Some(ArchiveProbe::supported(
            archive_family_for_format(detected_format),
            Some(detected_format),
        ))
    }

    pub fn resolve_for_path(&self, path: &Path) -> Result<Arc<dyn ArchiveBackend>, ArchiveError> {
        if let Some(probe) = self.probe_path(path) {
            if let ArchiveSupport::NotSupportedYet { reason } = probe.support {
                return Err(ArchiveError::FeatureNotSupported {
                    backend: "archive probe".into(),
                    feature: reason,
                });
            }
        }

        self.backends
            .iter()
            .find(|backend| backend.can_open(path))
            .cloned()
            .ok_or_else(|| {
                if self.probe_path(path).is_some() {
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
