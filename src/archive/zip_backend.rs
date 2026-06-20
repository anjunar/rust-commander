use std::{
    fs::{self, File},
    io,
    path::Path,
    time::SystemTime,
};

use zip::ZipArchive;

use super::{
    safe_join_extract_path, ArchiveBackend, ArchiveEntry, ArchiveEntryKind, ArchiveError,
    ArchiveFormat, ArchiveFormatDetector, ArchiveSession,
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
            modified_time: Self::modified_time(file),
            attributes: file.unix_mode().map(|mode| format!("{mode:o}")),
        })
    }

    fn modified_time(file: &zip::read::ZipFile<'_>) -> Option<SystemTime> {
        let timestamp = file.last_modified();
        let date = chrono::NaiveDate::from_ymd_opt(
            timestamp.year().into(),
            timestamp.month().into(),
            timestamp.day().into(),
        )?;
        let datetime = date.and_hms_opt(
            timestamp.hour().into(),
            timestamp.minute().into(),
            timestamp.second().into(),
        )?;
        Some(datetime.and_utc().into())
    }

    fn extract_selected(
        &self,
        session: &ArchiveSession,
        target_dir: &Path,
        predicate: impl Fn(&str) -> bool,
    ) -> Result<(), ArchiveError> {
        let mut archive = self.open_archive(session.archive_path())?;
        for index in 0..archive.len() {
            let mut file =
                archive
                    .by_index(index)
                    .map_err(|error| ArchiveError::ExtractionFailed {
                        path: session.archive_path().to_path_buf(),
                        detail: error.to_string(),
                    })?;
            let archive_path = Self::normalize_entry_path(file.name());
            if archive_path.is_empty() || !predicate(&archive_path) {
                continue;
            }

            let destination = safe_join_extract_path(target_dir, &archive_path)?;
            if file.is_dir() {
                fs::create_dir_all(&destination).map_err(|error| {
                    ArchiveError::ExtractionFailed {
                        path: session.archive_path().to_path_buf(),
                        detail: format!("Could not create {}: {error}", destination.display()),
                    }
                })?;
                continue;
            }

            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|error| ArchiveError::ExtractionFailed {
                    path: session.archive_path().to_path_buf(),
                    detail: format!("Could not create {}: {error}", parent.display()),
                })?;
            }

            let mut output =
                File::create(&destination).map_err(|error| ArchiveError::ExtractionFailed {
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

    fn can_open(&self, path: &Path) -> bool {
        matches!(
            ArchiveFormatDetector::detect(path),
            Some(ArchiveFormat::Zip)
        )
    }

    fn open(&self, path: &Path) -> Result<ArchiveSession, ArchiveError> {
        let mut archive = self.open_archive(path)?;
        let mut entries = Vec::new();
        for index in 0..archive.len() {
            let file = archive
                .by_index(index)
                .map_err(|error| ArchiveError::ListFailed {
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
            entries,
        ))
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
            requested
                .iter()
                .any(|path| candidate == path || candidate.starts_with(&format!("{path}/")))
        })
    }
}
