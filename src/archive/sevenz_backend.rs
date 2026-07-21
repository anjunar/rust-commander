use std::{
    fs::{self, File},
    io::{self, Read},
    path::Path,
    time::SystemTime,
};

use sevenz_rust2::{ArchiveEntry as SevenZEntry, ArchiveReader, Error as SevenZError, Password};

use super::{
    archive_entry_display_name, archive_entry_matches_selection, normalize_archive_entry_path,
    safe_join_extract_path, ArchiveBackend, ArchiveEntry, ArchiveEntryKind, ArchiveError,
    ArchiveFormatDetector, ArchiveSession,
};

#[derive(Clone, Debug, Default)]
pub struct SevenZipBackend;

impl SevenZipBackend {
    pub fn new() -> Self {
        Self
    }

    pub(crate) fn open_in_process(&self, path: &Path) -> Result<ArchiveSession, ArchiveError> {
        if !self.can_open(path) {
            return Err(ArchiveError::UnsupportedFormat {
                path: path.to_path_buf(),
            });
        }

        let reader = self.open_reader(path)?;
        let entries = reader
            .archive()
            .files
            .iter()
            .filter_map(Self::entry_from_sevenz)
            .collect();

        Ok(ArchiveSession::new(
            self.id(),
            path.to_path_buf(),
            entries,
        ))
    }

    pub(crate) fn extract_entries_in_process(
        &self,
        session: &ArchiveSession,
        entry_paths: &[String],
        target_dir: &Path,
    ) -> Result<(), ArchiveError> {
        let normalized = entry_paths
            .iter()
            .map(|path| normalize_archive_entry_path(path))
            .collect::<Vec<_>>();
        self.extract_matching(session, target_dir, |candidate| {
            archive_entry_matches_selection(candidate, &normalized)
        })
    }

    fn open_reader(&self, path: &Path) -> Result<ArchiveReader<File>, ArchiveError> {
        ArchiveReader::open(path, Password::empty()).map_err(|error| map_sevenz_error(path, error))
    }

    fn entry_from_sevenz(entry: &SevenZEntry) -> Option<ArchiveEntry> {
        if entry.is_anti_item() {
            return None;
        }

        let archive_path = normalize_archive_entry_path(entry.name());
        if archive_path.is_empty() {
            return None;
        }

        let kind = if entry.is_directory() {
            ArchiveEntryKind::Directory
        } else {
            ArchiveEntryKind::File
        };
        let modified_time = entry
            .has_last_modified_date
            .then(|| SystemTime::from(entry.last_modified_date()));
        let attributes = entry
            .has_windows_attributes
            .then(|| format!("{:X}", entry.windows_attributes()));

        Some(ArchiveEntry {
            display_name: archive_entry_display_name(&archive_path),
            archive_path,
            kind,
            size: entry.size(),
            modified_time,
            attributes,
        })
    }

    fn extract_matching(
        &self,
        session: &ArchiveSession,
        target_dir: &Path,
        should_extract: impl Fn(&str) -> bool,
    ) -> Result<(), ArchiveError> {
        let mut archive = self.open_reader(session.archive_path())?;

        archive
            .for_each_entries(|entry, reader| {
                if entry.is_anti_item() {
                    drain_reader(reader)?;
                    return Ok(true);
                }

                let archive_path = normalize_archive_entry_path(entry.name());
                if archive_path.is_empty() || !should_extract(&archive_path) {
                    // Solid archives require consuming preceding entry data.
                    drain_reader(reader)?;
                    return Ok(true);
                }

                let destination = match safe_join_extract_path(target_dir, &archive_path) {
                    Ok(path) => path,
                    Err(error) => return Err(SevenZError::Other(error.to_string().into())),
                };

                if entry.is_directory() {
                    fs::create_dir_all(&destination)
                        .map_err(|error| io_create_error(&destination, error))?;
                    return Ok(true);
                }

                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent).map_err(|error| io_create_error(parent, error))?;
                }

                let mut output = File::create(&destination).map_err(|error| {
                    SevenZError::FileOpen(error, destination.to_string_lossy().into_owned())
                })?;
                io::copy(reader, &mut output).map_err(SevenZError::from)?;
                Ok(true)
            })
            .map_err(|error| map_sevenz_error(session.archive_path(), error))?;

        Ok(())
    }
}

impl ArchiveBackend for SevenZipBackend {
    fn id(&self) -> &'static str {
        "sevenzip"
    }

    fn name(&self) -> &'static str {
        "7-Zip backend"
    }

    fn priority(&self) -> u32 {
        200
    }

    fn can_open(&self, path: &Path) -> bool {
        ArchiveFormatDetector::is_seven_zip_path(path)
    }

    fn open(&self, path: &Path) -> Result<ArchiveSession, ArchiveError> {
        // Windows UI process: list out-of-process (see sevenz_worker). Tests use
        // the in-process reader directly.
        #[cfg(all(target_os = "windows", not(test)))]
        {
            let entries = super::sevenz_worker::list_via_helper(path)?;
            Ok(ArchiveSession::new(
                self.id(),
                path.to_path_buf(),
                entries,
            ))
        }

        #[cfg(any(not(target_os = "windows"), test))]
        {
            self.open_in_process(path)
        }
    }

    fn extract_entry(
        &self,
        session: &ArchiveSession,
        entry_path: &str,
        target_dir: &Path,
    ) -> Result<(), ArchiveError> {
        self.extract_entries(session, &[entry_path.to_string()], target_dir)
    }

    fn extract_entries(
        &self,
        session: &ArchiveSession,
        entry_paths: &[String],
        target_dir: &Path,
    ) -> Result<(), ArchiveError> {
        #[cfg(all(target_os = "windows", not(test)))]
        {
            super::sevenz_worker::extract_via_helper(
                session.archive_path(),
                target_dir,
                entry_paths,
            )
        }

        #[cfg(any(not(target_os = "windows"), test))]
        {
            self.extract_entries_in_process(session, entry_paths, target_dir)
        }
    }
}

fn drain_reader(reader: &mut dyn Read) -> Result<(), SevenZError> {
    io::copy(reader, &mut io::sink())
        .map(|_| ())
        .map_err(SevenZError::from)
}

fn io_create_error(path: &Path, error: io::Error) -> SevenZError {
    SevenZError::Other(format!("Could not create {}: {error}", path.display()).into())
}

fn map_sevenz_error(path: &Path, error: SevenZError) -> ArchiveError {
    match error {
        SevenZError::PasswordRequired => ArchiveError::PasswordRequired {
            path: path.to_path_buf(),
        },
        SevenZError::MaybeBadPassword(_) => ArchiveError::WrongPassword {
            path: path.to_path_buf(),
        },
        SevenZError::BadSignature(_)
        | SevenZError::UnsupportedVersion { .. }
        | SevenZError::ChecksumVerificationFailed
        | SevenZError::NextHeaderCrcMismatch
        | SevenZError::BadTerminatedStreamsInfo(_)
        | SevenZError::BadTerminatedUnpackInfo
        | SevenZError::BadTerminatedPackInfo(_)
        | SevenZError::BadTerminatedSubStreamsInfo
        | SevenZError::BadTerminatedHeader(_) => ArchiveError::InvalidArchive {
            path: path.to_path_buf(),
            detail: Some(error.to_string()),
        },
        SevenZError::FileOpen(io_error, detail) => ArchiveError::IoError {
            detail: format!(
                "Could not open archive {}: {detail}: {io_error}",
                path.display()
            ),
        },
        SevenZError::Io(io_error, detail) => ArchiveError::IoError {
            detail: format!("7z I/O error for {}: {detail}: {io_error}", path.display()),
        },
        SevenZError::Other(detail) => ArchiveError::ExtractionFailed {
            path: path.to_path_buf(),
            detail: detail.into_owned(),
        },
        SevenZError::UnsupportedCompressionMethod(_)
        | SevenZError::ExternalUnsupported
        | SevenZError::Unsupported(_)
        | SevenZError::MaxMemLimited { .. }
        | SevenZError::FileNotFound => ArchiveError::LibraryError {
            library: "sevenz-rust2".into(),
            detail: error.to_string(),
        },
    }
}

#[cfg(test)]
#[path = "../../tests/unit/archive_sevenz_tests.rs"]
mod tests;
