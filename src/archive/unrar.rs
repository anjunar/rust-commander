use std::{fs, path::Path};

use unrar::{
    error::{Code, UnrarError, When},
    Archive, CursorBeforeHeader, OpenArchive, Process,
};

use super::{
    safe_join_extract_path, ArchiveBackend, ArchiveEntry, ArchiveEntryKind, ArchiveError,
    ArchiveFormat, ArchiveFormatDetector, ArchiveSession,
};

#[derive(Clone, Debug, Default)]
pub struct UnrarBackend;

impl UnrarBackend {
    pub fn new() -> Self {
        Self
    }

    fn processing_archive(
        &self,
        path: &Path,
    ) -> Result<OpenArchive<Process, CursorBeforeHeader>, ArchiveError> {
        Archive::new(path)
            .as_first_part()
            .open_for_processing()
            .map_err(|error| map_unrar_error(path, error))
    }

    fn normalized_path(path: &Path) -> String {
        path.to_string_lossy()
            .replace('\\', "/")
            .trim_matches('/')
            .to_string()
    }

    fn entry_from_header(header: &unrar::FileHeader) -> Option<ArchiveEntry> {
        let archive_path = Self::normalized_path(&header.filename);
        if archive_path.is_empty() {
            return None;
        }

        let display_name = header
            .filename
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| archive_path.clone());
        let kind = if header.is_directory() {
            ArchiveEntryKind::Directory
        } else {
            ArchiveEntryKind::File
        };

        Some(ArchiveEntry {
            archive_path,
            display_name,
            kind,
            size: header.unpacked_size,
            modified_time: None,
            attributes: Some(format!("{:X}", header.file_attr)),
        })
    }

    fn selected_match(candidate: &str, selection: &[String]) -> bool {
        selection
            .iter()
            .any(|selected| candidate == selected || candidate.starts_with(&format!("{selected}/")))
    }

    fn extract_matching(
        &self,
        session: &ArchiveSession,
        target_dir: &Path,
        should_extract: impl Fn(&str) -> bool,
    ) -> Result<(), ArchiveError> {
        let mut archive = self.processing_archive(session.archive_path())?;

        while let Some(header) = archive
            .read_header()
            .map_err(|error| map_unrar_error(session.archive_path(), error))?
        {
            let entry = header.entry();
            let archive_path = Self::normalized_path(&entry.filename);
            if archive_path.is_empty() || !should_extract(&archive_path) {
                archive = header
                    .skip()
                    .map_err(|error| map_unrar_error(session.archive_path(), error))?;
                continue;
            }

            let destination = safe_join_extract_path(target_dir, &archive_path)?;
            if entry.is_directory() {
                fs::create_dir_all(&destination).map_err(|error| {
                    ArchiveError::ExtractionFailed {
                        path: session.archive_path().to_path_buf(),
                        detail: format!("Could not create {}: {error}", destination.display()),
                    }
                })?;
                archive = header
                    .skip()
                    .map_err(|error| map_unrar_error(session.archive_path(), error))?;
                continue;
            }

            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|error| ArchiveError::ExtractionFailed {
                    path: session.archive_path().to_path_buf(),
                    detail: format!("Could not create {}: {error}", parent.display()),
                })?;
            }

            archive = header
                .extract_to(&destination)
                .map_err(|error| map_unrar_error(session.archive_path(), error))?;
        }

        Ok(())
    }
}

impl ArchiveBackend for UnrarBackend {
    fn id(&self) -> &'static str {
        "unrar"
    }

    fn name(&self) -> &'static str {
        "UnRAR backend"
    }

    fn priority(&self) -> u32 {
        180
    }

    fn can_open(&self, path: &Path) -> bool {
        matches!(
            ArchiveFormatDetector::detect(path),
            Some(ArchiveFormat::Rar)
        )
    }

    fn open(&self, path: &Path) -> Result<ArchiveSession, ArchiveError> {
        if !self.can_open(path) {
            return Err(ArchiveError::UnsupportedFormat {
                path: path.to_path_buf(),
            });
        }

        let mut archive = Archive::new(path)
            .as_first_part()
            .open_for_listing()
            .map_err(|error| map_unrar_error(path, error))?;
        let mut entries = Vec::new();

        for header in &mut archive {
            let header = header.map_err(|error| ArchiveError::ListFailed {
                path: path.to_path_buf(),
                detail: map_unrar_error(path, error).to_string(),
            })?;
            if let Some(entry) = Self::entry_from_header(&header) {
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
        let requested = Self::normalized_path(Path::new(entry_path));
        self.extract_matching(session, target_dir, |candidate| {
            candidate == requested || candidate.starts_with(&format!("{requested}/"))
        })
    }

    fn extract_entries(
        &self,
        session: &ArchiveSession,
        entry_paths: &[String],
        target_dir: &Path,
    ) -> Result<(), ArchiveError> {
        let normalized = entry_paths
            .iter()
            .map(|path| Self::normalized_path(Path::new(path)))
            .collect::<Vec<_>>();
        self.extract_matching(session, target_dir, |candidate| {
            Self::selected_match(candidate, &normalized)
        })
    }
}

fn map_unrar_error(path: &Path, error: UnrarError) -> ArchiveError {
    match error.code {
        Code::MissingPassword => ArchiveError::PasswordRequired {
            path: path.to_path_buf(),
        },
        Code::BadPassword => ArchiveError::WrongPassword {
            path: path.to_path_buf(),
        },
        Code::BadArchive | Code::BadData | Code::UnknownFormat => ArchiveError::InvalidArchive {
            path: path.to_path_buf(),
            detail: Some(error.to_string()),
        },
        Code::EOpen | Code::ERead => ArchiveError::IoError {
            detail: format!("RAR I/O error for {}: {}", path.display(), error),
        },
        Code::ECreate | Code::EWrite | Code::EClose => ArchiveError::ExtractionFailed {
            path: path.to_path_buf(),
            detail: error.to_string(),
        },
        Code::NoMemory
        | Code::SmallBuf
        | Code::Unknown
        | Code::EReference
        | Code::EndArchive
        | Code::Success => match error.when {
            When::Open | When::Read => ArchiveError::LibraryError {
                library: "unrar".into(),
                detail: error.to_string(),
            },
            When::Process => ArchiveError::ExtractionFailed {
                path: path.to_path_buf(),
                detail: error.to_string(),
            },
        },
    }
}

#[cfg(test)]
#[path = "../../tests/unit/archive_unrar_tests.rs"]
mod tests;
