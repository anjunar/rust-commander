#[cfg(target_os = "windows")]
use std::path::Path;

#[cfg(not(target_os = "windows"))]
use super::{safe_join_extract_path, ArchiveEntryKind};
use super::{
    ArchiveBackend, ArchiveCapabilities, ArchiveEntry, ArchiveError, ArchiveFormat,
    ArchiveFormatDetector, ArchiveSession,
};

#[derive(Clone, Debug, Default)]
pub struct LibArchiveBackend;

impl LibArchiveBackend {
    pub fn new() -> Self {
        Self
    }

    #[cfg(target_os = "windows")]
    fn unsupported(path: &Path) -> ArchiveError {
        ArchiveError::FeatureNotSupported {
            backend: "libarchive".into(),
            feature: format!("Opening {} via native libarchive backend", path.display()),
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn supports_format(format: ArchiveFormat) -> bool {
        matches!(
            format,
            ArchiveFormat::Tar
                | ArchiveFormat::TarGz
                | ArchiveFormat::TarBz2
                | ArchiveFormat::TarXz
                | ArchiveFormat::Gz
                | ArchiveFormat::Bz2
                | ArchiveFormat::Xz
                | ArchiveFormat::Cab
                | ArchiveFormat::Iso
                | ArchiveFormat::Wim
                | ArchiveFormat::Cpio
        )
    }
}

#[cfg(not(target_os = "windows"))]
mod native {
    use std::{
        fs::{self, File},
        io::Write,
        path::Path,
    };

    use libarchive::{
        archive::{Entry, FileType, ReadFilter, ReadFormat},
        reader::{Builder, Reader},
    };

    use super::*;

    impl LibArchiveBackend {
        fn open_reader(&self, path: &Path) -> Result<libarchive::reader::FileReader, ArchiveError> {
            let mut builder = Builder::new();
            builder.support_filter(ReadFilter::All).map_err(|error| {
                ArchiveError::LibraryError {
                    library: "libarchive".into(),
                    detail: error.to_string(),
                }
            })?;
            builder.support_format(ReadFormat::All).map_err(|error| {
                ArchiveError::LibraryError {
                    library: "libarchive".into(),
                    detail: error.to_string(),
                }
            })?;
            builder
                .open_file(path)
                .map_err(|error| ArchiveError::InvalidArchive {
                    path: path.to_path_buf(),
                    detail: Some(error.to_string()),
                })
        }

        fn list_archive_entries(&self, path: &Path) -> Result<Vec<ArchiveEntry>, ArchiveError> {
            let mut reader = self.open_reader(path)?;
            let mut entries = Vec::new();

            while let Some(header) = reader.next_header() {
                let archive_path = normalize_archive_path(header.pathname());
                if archive_path.is_empty() {
                    continue;
                }

                entries.push(ArchiveEntry {
                    display_name: archive_path
                        .rsplit('/')
                        .next()
                        .unwrap_or(&archive_path)
                        .to_string(),
                    archive_path,
                    kind: entry_kind(header.filetype()),
                    size: header.size().max(0) as u64,
                    packed_size: None,
                    modified_time: None,
                    crc: None,
                    encrypted: false,
                    method: Some("libarchive".into()),
                    attributes: None,
                });
            }

            Ok(entries)
        }

        fn extract_matching(
            &self,
            session: &ArchiveSession,
            target_dir: &Path,
            should_extract: impl Fn(&str) -> bool,
        ) -> Result<(), ArchiveError> {
            let mut reader = self.open_reader(session.archive_path())?;

            while let Some(header) = reader.next_header() {
                let archive_path = normalize_archive_path(header.pathname());
                if archive_path.is_empty() || !should_extract(&archive_path) {
                    continue;
                }

                let destination = safe_join_extract_path(target_dir, &archive_path)?;
                match entry_kind(header.filetype()) {
                    ArchiveEntryKind::Directory => {
                        fs::create_dir_all(&destination).map_err(|error| {
                            ArchiveError::ExtractionFailed {
                                path: session.archive_path().to_path_buf(),
                                detail: format!(
                                    "Could not create {}: {error}",
                                    destination.display()
                                ),
                            }
                        })?;
                    }
                    ArchiveEntryKind::File | ArchiveEntryKind::Unknown => {
                        if let Some(parent) = destination.parent() {
                            fs::create_dir_all(parent).map_err(|error| {
                                ArchiveError::ExtractionFailed {
                                    path: session.archive_path().to_path_buf(),
                                    detail: format!(
                                        "Could not create {}: {error}",
                                        parent.display()
                                    ),
                                }
                            })?;
                        }

                        let mut output = File::create(&destination).map_err(|error| {
                            ArchiveError::ExtractionFailed {
                                path: session.archive_path().to_path_buf(),
                                detail: format!(
                                    "Could not create {}: {error}",
                                    destination.display()
                                ),
                            }
                        })?;

                        while let Some(block) =
                            reader
                                .read_block()
                                .map_err(|error| ArchiveError::ExtractionFailed {
                                    path: session.archive_path().to_path_buf(),
                                    detail: error.to_string(),
                                })?
                        {
                            output.write_all(block).map_err(|error| {
                                ArchiveError::ExtractionFailed {
                                    path: session.archive_path().to_path_buf(),
                                    detail: format!(
                                        "Could not write {}: {error}",
                                        destination.display()
                                    ),
                                }
                            })?;
                        }
                    }
                    ArchiveEntryKind::Symlink => {
                        #[cfg(unix)]
                        {
                            if let Some(parent) = destination.parent() {
                                fs::create_dir_all(parent).map_err(|error| {
                                    ArchiveError::ExtractionFailed {
                                        path: session.archive_path().to_path_buf(),
                                        detail: format!(
                                            "Could not create {}: {error}",
                                            parent.display()
                                        ),
                                    }
                                })?;
                            }

                            let link_target = header.symlink().to_string();
                            std::os::unix::fs::symlink(&link_target, &destination).map_err(
                                |error| ArchiveError::ExtractionFailed {
                                    path: session.archive_path().to_path_buf(),
                                    detail: format!(
                                        "Could not create symlink {} -> {}: {error}",
                                        destination.display(),
                                        link_target
                                    ),
                                },
                            )?;
                        }

                        #[cfg(not(unix))]
                        {
                            return Err(ArchiveError::FeatureNotSupported {
                                backend: "libarchive".into(),
                                feature: format!(
                                    "Extracting symlinks from {} on this platform",
                                    session.archive_path().display()
                                ),
                            });
                        }
                    }
                }
            }

            Ok(())
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
            280
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

        fn can_open(&self, path: &Path) -> bool {
            ArchiveFormatDetector::detect(path).is_some_and(LibArchiveBackend::supports_format)
        }

        fn open(&self, path: &Path) -> Result<ArchiveSession, ArchiveError> {
            let detected = ArchiveFormatDetector::detect(path);
            if !detected.is_some_and(LibArchiveBackend::supports_format) {
                return Err(ArchiveError::UnsupportedFormat {
                    path: path.to_path_buf(),
                });
            }

            let entries = self.list_archive_entries(path)?;
            Ok(ArchiveSession::new(
                self.id(),
                path.to_path_buf(),
                detected,
                entries,
                self.capabilities(),
            ))
        }

        fn list_entries(
            &self,
            session: &ArchiveSession,
        ) -> Result<Vec<ArchiveEntry>, ArchiveError> {
            Ok(session.cached_entries().to_vec())
        }

        fn extract_entry(
            &self,
            session: &ArchiveSession,
            entry_path: &str,
            target_dir: &Path,
        ) -> Result<(), ArchiveError> {
            self.extract_matching(session, target_dir, |candidate| {
                candidate == entry_path || candidate.starts_with(&format!("{entry_path}/"))
            })
        }

        fn extract_entries(
            &self,
            session: &ArchiveSession,
            entry_paths: &[String],
            target_dir: &Path,
        ) -> Result<(), ArchiveError> {
            self.extract_matching(session, target_dir, |candidate| {
                entry_paths
                    .iter()
                    .any(|path| candidate == path || candidate.starts_with(&format!("{path}/")))
            })
        }

        fn extract_all(
            &self,
            session: &ArchiveSession,
            target_dir: &Path,
        ) -> Result<(), ArchiveError> {
            self.extract_matching(session, target_dir, |_| true)
        }

        fn test_archive(&self, session: &ArchiveSession) -> Result<(), ArchiveError> {
            let mut reader = self.open_reader(session.archive_path())?;
            while let Some(_header) = reader.next_header() {
                while let Some(_block) =
                    reader
                        .read_block()
                        .map_err(|error| ArchiveError::InvalidArchive {
                            path: session.archive_path().to_path_buf(),
                            detail: Some(error.to_string()),
                        })?
                {}
            }
            Ok(())
        }
    }

    fn normalize_archive_path(path: &str) -> String {
        path.replace('\\', "/").trim_matches('/').to_string()
    }

    fn entry_kind(file_type: FileType) -> ArchiveEntryKind {
        match file_type {
            FileType::Directory => ArchiveEntryKind::Directory,
            FileType::RegularFile => ArchiveEntryKind::File,
            FileType::SymbolicLink => ArchiveEntryKind::Symlink,
            _ => ArchiveEntryKind::Unknown,
        }
    }
}

#[cfg(target_os = "windows")]
impl ArchiveBackend for LibArchiveBackend {
    fn id(&self) -> &'static str {
        "libarchive"
    }

    fn name(&self) -> &'static str {
        "libarchive backend"
    }

    fn priority(&self) -> u32 {
        280
    }

    fn supported_extensions(&self) -> &'static [&'static str] {
        &[
            "tar", "tar.gz", "tgz", "tar.bz2", "tbz2", "tar.xz", "txz", "gz", "bz2", "xz", "cab",
            "iso", "wim", "cpio",
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
        let detected = ArchiveFormatDetector::detect(path).or(Some(ArchiveFormat::Tar));
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
