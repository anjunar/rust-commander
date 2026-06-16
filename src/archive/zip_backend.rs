use std::{
    fs::{self, File},
    io,
    path::Path,
};

use zip::ZipArchive;

use super::{
    safe_join_extract_path, ArchiveBackend, ArchiveCapabilities, ArchiveEntry, ArchiveEntryKind,
    ArchiveError, ArchiveFormat, ArchiveFormatDetector, ArchiveSession,
};

#[derive(Clone, Debug, Default)]
pub struct ZipBackend;

impl ZipBackend {
    pub fn new() -> Self {
        Self
    }

    fn open_archive(&self, path: &Path) -> Result<ZipArchive<File>, ArchiveError> {
        let file = File::open(path).map_err(|error| ArchiveError::IoError {
            detail: format!("Could not open archive {}: {error}", path.display()),
        })?;
        ZipArchive::new(file).map_err(|error| ArchiveError::InvalidArchive {
            path: path.to_path_buf(),
            detail: Some(error.to_string()),
        })
    }

    fn normalize_entry_path(path: &str) -> String {
        path.replace('\\', "/").trim_matches('/').to_string()
    }

    fn entry_kind(file: &zip::read::ZipFile<'_>) -> ArchiveEntryKind {
        if file.is_dir() {
            ArchiveEntryKind::Directory
        } else {
            ArchiveEntryKind::File
        }
    }

    fn entry_from_zip_file(file: &zip::read::ZipFile<'_>) -> Option<ArchiveEntry> {
        let archive_path = Self::normalize_entry_path(file.name());
        if archive_path.is_empty() {
            return None;
        }

        let display_name = archive_path
            .rsplit('/')
            .next()
            .unwrap_or(&archive_path)
            .to_string();
        Some(ArchiveEntry {
            archive_path,
            display_name,
            kind: Self::entry_kind(file),
            size: file.size(),
            packed_size: Some(file.compressed_size()),
            modified_time: None,
            crc: Some(format!("{:08X}", file.crc32())),
            encrypted: false,
            method: Some(format!("{:?}", file.compression())),
            attributes: file.unix_mode().map(|mode| format!("{mode:o}")),
        })
    }

    fn extract_selected(
        &self,
        session: &ArchiveSession,
        target_dir: &Path,
        predicate: impl Fn(&str) -> bool,
    ) -> Result<(), ArchiveError> {
        let mut archive = self.open_archive(session.archive_path())?;
        for index in 0..archive.len() {
            let mut file = archive.by_index(index).map_err(|error| ArchiveError::ExtractionFailed {
                path: session.archive_path().to_path_buf(),
                detail: error.to_string(),
            })?;
            let archive_path = Self::normalize_entry_path(file.name());
            if archive_path.is_empty() || !predicate(&archive_path) {
                continue;
            }

            let destination = safe_join_extract_path(target_dir, &archive_path)?;
            if file.is_dir() {
                fs::create_dir_all(&destination).map_err(|error| ArchiveError::ExtractionFailed {
                    path: session.archive_path().to_path_buf(),
                    detail: format!("Could not create {}: {error}", destination.display()),
                })?;
                continue;
            }

            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|error| ArchiveError::ExtractionFailed {
                    path: session.archive_path().to_path_buf(),
                    detail: format!("Could not create {}: {error}", parent.display()),
                })?;
            }

            let mut output = File::create(&destination).map_err(|error| ArchiveError::ExtractionFailed {
                path: session.archive_path().to_path_buf(),
                detail: format!("Could not create {}: {error}", destination.display()),
            })?;
            io::copy(&mut file, &mut output).map_err(|error| ArchiveError::ExtractionFailed {
                path: session.archive_path().to_path_buf(),
                detail: format!("Could not write {}: {error}", destination.display()),
            })?;
        }
        Ok(())
    }
}

impl ArchiveBackend for ZipBackend {
    fn id(&self) -> &'static str {
        "zip"
    }

    fn name(&self) -> &'static str {
        "ZIP backend"
    }

    fn priority(&self) -> u32 {
        300
    }

    fn supported_extensions(&self) -> &'static [&'static str] {
        &["zip"]
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

    fn can_open(&self, path: &Path) -> bool {
        matches!(ArchiveFormatDetector::detect(path), Some(ArchiveFormat::Zip))
    }

    fn open(&self, path: &Path) -> Result<ArchiveSession, ArchiveError> {
        let mut archive = self.open_archive(path)?;
        let mut entries = Vec::new();
        for index in 0..archive.len() {
            let file = archive.by_index(index).map_err(|error| ArchiveError::ListFailed {
                path: path.to_path_buf(),
                detail: error.to_string(),
            })?;
            if let Some(entry) = Self::entry_from_zip_file(&file) {
                entries.push(entry);
            }
        }

        Ok(ArchiveSession::new(
            self.id(),
            path.to_path_buf(),
            ArchiveFormatDetector::detect(path),
            entries,
            self.capabilities(),
        ))
    }

    fn list_entries(&self, session: &ArchiveSession) -> Result<Vec<ArchiveEntry>, ArchiveError> {
        Ok(session.cached_entries().to_vec())
    }

    fn extract_entry(
        &self,
        session: &ArchiveSession,
        entry_path: &str,
        target_dir: &Path,
    ) -> Result<(), ArchiveError> {
        let requested = Self::normalize_entry_path(entry_path);
        self.extract_selected(session, target_dir, |candidate| {
            candidate == requested || candidate.starts_with(&format!("{requested}/"))
        })
    }

    fn extract_entries(
        &self,
        session: &ArchiveSession,
        entry_paths: &[String],
        target_dir: &Path,
    ) -> Result<(), ArchiveError> {
        let requested = entry_paths
            .iter()
            .map(|path| Self::normalize_entry_path(path))
            .collect::<Vec<_>>();
        self.extract_selected(session, target_dir, |candidate| {
            requested.iter().any(|path| candidate == path || candidate.starts_with(&format!("{path}/")))
        })
    }

    fn extract_all(
        &self,
        session: &ArchiveSession,
        target_dir: &Path,
    ) -> Result<(), ArchiveError> {
        self.extract_selected(session, target_dir, |_| true)
    }

    fn test_archive(&self, session: &ArchiveSession) -> Result<(), ArchiveError> {
        let mut archive = self.open_archive(session.archive_path())?;
        for index in 0..archive.len() {
            let mut file = archive.by_index(index).map_err(|error| ArchiveError::InvalidArchive {
                path: session.archive_path().to_path_buf(),
                detail: Some(error.to_string()),
            })?;
            io::copy(&mut file, &mut io::sink()).map_err(|error| ArchiveError::InvalidArchive {
                path: session.archive_path().to_path_buf(),
                detail: Some(error.to_string()),
            })?;
        }
        Ok(())
    }
}
