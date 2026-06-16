use std::path::Path;

use super::{
    ArchiveBackend, ArchiveCapabilities, ArchiveEntry, ArchiveError, ArchiveSession,
};

#[derive(Clone, Debug, Default)]
pub struct PluginArchiveBackend;

impl PluginArchiveBackend {
    pub fn new_stub() -> Self {
        Self
    }

    fn unsupported(feature: &str) -> ArchiveError {
        ArchiveError::FeatureNotSupported {
            backend: "plugin".into(),
            feature: feature.into(),
        }
    }
}

impl ArchiveBackend for PluginArchiveBackend {
    fn id(&self) -> &'static str {
        "plugin"
    }

    fn name(&self) -> &'static str {
        "Plugin backend"
    }

    fn priority(&self) -> u32 {
        50
    }

    fn supported_extensions(&self) -> &'static [&'static str] {
        &[]
    }

    fn capabilities(&self) -> ArchiveCapabilities {
        ArchiveCapabilities::default()
    }

    fn can_open(&self, _path: &Path) -> bool {
        false
    }

    fn open(&self, _path: &Path) -> Result<ArchiveSession, ArchiveError> {
        Err(Self::unsupported("Archive plugins are not implemented yet"))
    }

    fn list_entries(&self, _session: &ArchiveSession) -> Result<Vec<ArchiveEntry>, ArchiveError> {
        Err(Self::unsupported("Archive plugins are not implemented yet"))
    }

    fn extract_entry(
        &self,
        _session: &ArchiveSession,
        _entry_path: &str,
        _target_dir: &Path,
    ) -> Result<(), ArchiveError> {
        Err(Self::unsupported("Archive plugins are not implemented yet"))
    }

    fn extract_entries(
        &self,
        _session: &ArchiveSession,
        _entry_paths: &[String],
        _target_dir: &Path,
    ) -> Result<(), ArchiveError> {
        Err(Self::unsupported("Archive plugins are not implemented yet"))
    }

    fn extract_all(
        &self,
        _session: &ArchiveSession,
        _target_dir: &Path,
    ) -> Result<(), ArchiveError> {
        Err(Self::unsupported("Archive plugins are not implemented yet"))
    }

    fn test_archive(&self, _session: &ArchiveSession) -> Result<(), ArchiveError> {
        Err(Self::unsupported("Archive plugins are not implemented yet"))
    }
}
