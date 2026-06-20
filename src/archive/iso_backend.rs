use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

use iso9660_simple::{ISODirectoryEntry, Read as IsoRead, ISO9660};

use super::{
    safe_join_extract_path, ArchiveBackend, ArchiveEntry, ArchiveEntryKind, ArchiveError,
    ArchiveFormat, ArchiveFormatDetector, ArchiveSession,
};

#[derive(Clone, Debug, Default)]
pub struct IsoBackend;

impl IsoBackend {
    pub fn new() -> Self {
        Self
    }

    fn open_iso(&self, path: &Path) -> Result<ISO9660, ArchiveError> {
        let file = File::open(path).map_err(|error| ArchiveError::IoError {
            detail: format!("Could not open ISO {}: {error}", path.display()),
        })?;

        ISO9660::from_device(IsoFileDevice(file)).ok_or_else(|| ArchiveError::FeatureNotSupported {
            backend: self.name().into(),
            feature: format!(
                "Opening non-ISO9660 or Apple/macOS hybrid ISO images like {}",
                path.display()
            ),
        })
    }

    fn list_iso_entries(&self, path: &Path) -> Result<Vec<ArchiveEntry>, ArchiveError> {
        let mut iso = self.open_iso(path)?;
        let root_lba = iso.root().lba.get();
        let mut entries = Vec::new();
        self.collect_entries(&mut iso, root_lba, "", &mut entries);
        Ok(entries)
    }

    fn collect_entries(
        &self,
        iso: &mut ISO9660,
        lba: u32,
        prefix: &str,
        entries: &mut Vec<ArchiveEntry>,
    ) {
        let iter = iso.read_directory(lba as usize);
        let directory_entries = (&iter).collect::<Vec<_>>();

        for directory_entry in directory_entries {
            let Some(name) = Self::normalized_name(&directory_entry.name) else {
                continue;
            };

            let archive_path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };

            let kind = if directory_entry.is_folder() {
                ArchiveEntryKind::Directory
            } else {
                ArchiveEntryKind::File
            };

            entries.push(ArchiveEntry {
                archive_path: archive_path.clone(),
                display_name: name,
                kind,
                size: directory_entry.file_size() as u64,
                modified_time: None,
                attributes: None,
            });

            if directory_entry.is_folder() {
                self.collect_entries(
                    iso,
                    directory_entry.record.lba.get(),
                    &archive_path,
                    entries,
                );
            }
        }
    }

    fn normalized_name(name: &str) -> Option<String> {
        if name == "." || name == ".." {
            return None;
        }

        let stripped = name
            .split_once(';')
            .map(|(base, _)| base)
            .unwrap_or(name)
            .trim_end_matches('.');

        if stripped.is_empty() {
            None
        } else {
            Some(stripped.to_string())
        }
    }

    fn find_entry_by_path(
        &self,
        iso: &mut ISO9660,
        archive_path: &str,
    ) -> Result<ISODirectoryEntry, ArchiveError> {
        let mut current_lba = iso.root().lba.get();
        let mut current_entry = None;

        for component in archive_path.split('/').filter(|part| !part.is_empty()) {
            let iter = iso.read_directory(current_lba as usize);
            let directory_entries = (&iter).collect::<Vec<_>>();

            let found = directory_entries
                .into_iter()
                .find(|entry| Self::normalized_name(&entry.name).as_deref() == Some(component))
                .ok_or_else(|| ArchiveError::ListFailed {
                    path: Path::new(archive_path).to_path_buf(),
                    detail: format!("ISO entry not found: {archive_path}"),
                })?;

            current_lba = found.record.lba.get();
            current_entry = Some(found);
        }

        current_entry.ok_or_else(|| ArchiveError::ListFailed {
            path: Path::new(archive_path).to_path_buf(),
            detail: format!("ISO entry not found: {archive_path}"),
        })
    }

    fn extract_entries_matching(
        &self,
        session: &ArchiveSession,
        target_dir: &Path,
        should_extract: impl Fn(&str) -> bool,
    ) -> Result<(), ArchiveError> {
        let mut iso = self.open_iso(session.archive_path())?;

        for entry in session
            .cached_entries()
            .iter()
            .filter(|entry| should_extract(&entry.archive_path))
        {
            let destination = safe_join_extract_path(target_dir, &entry.archive_path)?;
            if entry.kind == ArchiveEntryKind::Directory {
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

            let directory_entry = self.find_entry_by_path(&mut iso, &entry.archive_path)?;
            let mut content = vec![0_u8; directory_entry.file_size() as usize];
            let Some(read_len) = iso.read_file(&directory_entry, 0, &mut content) else {
                return Err(ArchiveError::ExtractionFailed {
                    path: session.archive_path().to_path_buf(),
                    detail: format!("Could not read {}", entry.archive_path),
                });
            };
            content.truncate(read_len);

            let mut output =
                File::create(&destination).map_err(|error| ArchiveError::ExtractionFailed {
                    path: session.archive_path().to_path_buf(),
                    detail: format!("Could not create {}: {error}", destination.display()),
                })?;
            output
                .write_all(&content)
                .map_err(|error| ArchiveError::ExtractionFailed {
                    path: session.archive_path().to_path_buf(),
                    detail: format!("Could not write {}: {error}", destination.display()),
                })?;
        }

        Ok(())
    }
}

impl ArchiveBackend for IsoBackend {
    fn id(&self) -> &'static str {
        "iso9660"
    }

    fn name(&self) -> &'static str {
        "ISO9660 backend"
    }

    fn priority(&self) -> u32 {
        250
    }

    fn can_open(&self, path: &Path) -> bool {
        matches!(
            ArchiveFormatDetector::detect(path),
            Some(ArchiveFormat::Iso)
        )
    }

    fn open(&self, path: &Path) -> Result<ArchiveSession, ArchiveError> {
        let entries = self.list_iso_entries(path)?;
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
        self.extract_entries_matching(session, target_dir, |candidate| {
            candidate == entry_path || candidate.starts_with(&format!("{entry_path}/"))
        })
    }

    fn extract_entries(
        &self,
        session: &ArchiveSession,
        entry_paths: &[String],
        target_dir: &Path,
    ) -> Result<(), ArchiveError> {
        self.extract_entries_matching(session, target_dir, |candidate| {
            entry_paths
                .iter()
                .any(|path| candidate == path || candidate.starts_with(&format!("{path}/")))
        })
    }
}

struct IsoFileDevice(File);

impl IsoRead for IsoFileDevice {
    fn read(&mut self, position: usize, buffer: &mut [u8]) -> Option<()> {
        self.0.seek(SeekFrom::Start(position as u64)).ok()?;
        self.0.read_exact(buffer).ok()?;
        Some(())
    }
}
