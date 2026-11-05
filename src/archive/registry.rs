use std::{cmp::Reverse, path::Path, sync::Arc};

use super::{
    probe::{archive_family_for_format, probe_multipart_zip},
    ArchiveBackend, ArchiveError, ArchiveFormatDetector, ArchiveProbe, ArchiveSupport, IsoBackend,
    LibArchiveBackend, PluginArchiveBackend, SevenZipBackend, UnrarBackend, ZipBackend,
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
            Arc::new(SevenZipBackend::new()),
            Arc::new(UnrarBackend::new()),
            Arc::new(PluginArchiveBackend::new_stub()),
        ])
    }

    pub fn is_archive_path(&self, path: &Path) -> bool {
        // Extension first: never open the file just to decide Enter behavior.
        ArchiveFormatDetector::detect_from_extension(path).is_some()
            || self.probe_path(path).is_some()
    }

    pub fn probe_path(&self, path: &Path) -> Option<ArchiveProbe> {
        if let Some(probe) = probe_multipart_zip(path) {
            return Some(probe);
        }

        let detected_format = ArchiveFormatDetector::detect(path)?;
        let family = archive_family_for_format(detected_format);
        if self.backend_that_can_open(path).is_some() {
            Some(ArchiveProbe::supported(family, Some(detected_format)))
        } else {
            Some(ArchiveProbe {
                family,
                detected_format: Some(detected_format),
                layout: super::probe::ArchiveLayout::SingleFile,
                support: ArchiveSupport::NotSupportedYet {
                    reason: format!("Opening {detected_format:?} archives is not supported yet"),
                },
            })
        }
    }

    /// Resolve a backend for opening. Uses extension/`can_open` only — no probe.
    pub fn backend_that_can_open(&self, path: &Path) -> Option<Arc<dyn ArchiveBackend>> {
        self.backends
            .iter()
            .find(|backend| backend.can_open(path))
            .cloned()
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
