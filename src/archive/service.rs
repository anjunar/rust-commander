use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
};

use super::{
    ArchiveBackend, ArchiveEntry, ArchiveEntryKind, ArchiveError, ArchiveFormatDetector,
    ArchiveOperation, ArchiveProgress, ArchiveSession,
};
use crate::{
    archive::safe_join_extract_path,
    domain::{Entry, PanelLocation},
    fs::reader::format_bytes,
};

#[derive(Clone)]
pub struct ArchiveService {
    backends: Arc<Vec<Arc<dyn ArchiveBackend>>>,
}

#[derive(Clone)]
pub struct ArchiveTaskHandle {
    cancelled: Arc<AtomicBool>,
}

impl ArchiveTaskHandle {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

#[derive(Clone, Debug)]
pub enum ArchiveTaskRequest {
    ExtractSelection {
        session: ArchiveSession,
        entry_paths: Vec<String>,
        target_dir: PathBuf,
    },
    ExtractAll {
        session: ArchiveSession,
        target_dir: PathBuf,
    },
    TestArchive {
        session: ArchiveSession,
    },
}

#[derive(Clone, Debug)]
pub enum ArchiveTaskEvent {
    Progress(ArchiveProgress),
    Finished(String),
    Failed(ArchiveError),
    Cancelled,
}

impl ArchiveService {
    pub fn new(backends: Vec<Arc<dyn ArchiveBackend>>) -> Self {
        Self {
            backends: Arc::new(backends),
        }
    }

    pub fn empty() -> Self {
        Self::new(Vec::new())
    }

    pub fn is_archive_path(&self, path: &Path) -> bool {
        ArchiveFormatDetector::is_supported_archive(path)
    }

    pub fn open_archive(&self, path: &Path) -> Result<ArchiveSession, ArchiveError> {
        let backend = self
            .backends
            .iter()
            .find(|backend| backend.can_open(path))
            .ok_or_else(|| {
                if ArchiveFormatDetector::is_supported_archive(path) {
                    ArchiveError::BackendNotFound {
                        backend: "archive backend".into(),
                        path: Some(path.to_path_buf()),
                    }
                } else {
                    ArchiveError::UnsupportedFormat {
                        path: path.to_path_buf(),
                    }
                }
            })?;

        backend.open(path)
    }

    pub fn entries_for_location(&self, location: &PanelLocation) -> Result<Vec<Entry>, ArchiveError> {
        match location {
            PanelLocation::Filesystem(path) => crate::fs::reader::read_entries(path)
                .map_err(|error| ArchiveError::IoError {
                    detail: error.to_string(),
                }),
            PanelLocation::Archive(view) => Ok(self.entries_for_archive_path(
                &view.session,
                &view.current_path,
            )),
        }
    }

    pub fn archive_location_for_path(&self, path: &Path) -> Result<PanelLocation, ArchiveError> {
        let session = self.open_archive(path)?;
        Ok(PanelLocation::archive(session, ""))
    }

    pub fn start_task(
        &self,
        request: ArchiveTaskRequest,
    ) -> (ArchiveTaskHandle, Receiver<ArchiveTaskEvent>) {
        let (tx, rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let handle = ArchiveTaskHandle {
            cancelled: Arc::clone(&cancelled),
        };
        let service = self.clone();

        thread::spawn(move || {
            let result = match request {
                ArchiveTaskRequest::ExtractSelection {
                    session,
                    entry_paths,
                    target_dir,
                } => service.extract_selection(&session, &entry_paths, &target_dir, &cancelled, &tx),
                ArchiveTaskRequest::ExtractAll {
                    session,
                    target_dir,
                } => service.extract_all(&session, &target_dir, &cancelled, &tx),
                ArchiveTaskRequest::TestArchive { session } => match service.backend_for_session(&session) {
                    Ok(backend) => {
                        let result = backend.test_archive(&session);
                        if result.is_ok() {
                            let _ = tx.send(ArchiveTaskEvent::Finished(format!(
                                "Archive test completed: {}",
                                session.archive_path().display()
                            )));
                        }
                        result
                    }
                    Err(error) => Err(error),
                },
            };

            if cancelled.load(Ordering::Relaxed) {
                let _ = tx.send(ArchiveTaskEvent::Cancelled);
            } else if let Err(error) = result {
                let _ = tx.send(ArchiveTaskEvent::Failed(error));
            }
        });

        (handle, rx)
    }

    pub fn backend_for_session(&self, session: &ArchiveSession) -> Result<&Arc<dyn ArchiveBackend>, ArchiveError> {
        self.backends
            .iter()
            .find(|backend| backend.id() == session.backend_id())
            .ok_or_else(|| ArchiveError::BackendNotFound {
                backend: session.backend_id().into(),
                path: Some(session.archive_path().to_path_buf()),
            })
    }

    fn extract_selection(
        &self,
        session: &ArchiveSession,
        entry_paths: &[String],
        target_dir: &Path,
        cancelled: &AtomicBool,
        tx: &mpsc::Sender<ArchiveTaskEvent>,
    ) -> Result<(), ArchiveError> {
        let all_entries = session.cached_entries();
        validate_selection_paths(target_dir, all_entries, entry_paths)?;
        if cancelled.load(Ordering::Relaxed) {
            return Ok(());
        }

        let backend = self.backend_for_session(session)?;
        let total_entries = entry_paths.len() as u64;

        for (index, entry_path) in entry_paths.iter().enumerate() {
            let _ = tx.send(ArchiveTaskEvent::Progress(ArchiveProgress {
                operation: Some(ArchiveOperation::ExtractEntry {
                    entry_path: entry_path.clone(),
                    target_dir: target_dir.to_path_buf(),
                }),
                current_path: Some(entry_path.clone()),
                processed_entries: Some(index as u64),
                total_entries: Some(total_entries),
                percent: Some(index as f64 / total_entries.max(1) as f64),
                ..ArchiveProgress::default()
            }));
            backend.extract_entry(session, entry_path, target_dir)?;
        }

        let _ = tx.send(ArchiveTaskEvent::Finished(format!(
            "Extracted {} item(s) to {}",
            entry_paths.len(),
            target_dir.display()
        )));
        Ok(())
    }

    fn extract_all(
        &self,
        session: &ArchiveSession,
        target_dir: &Path,
        cancelled: &AtomicBool,
        tx: &mpsc::Sender<ArchiveTaskEvent>,
    ) -> Result<(), ArchiveError> {
        validate_all_paths(target_dir, session.cached_entries())?;
        if cancelled.load(Ordering::Relaxed) {
            return Ok(());
        }

        let backend = self.backend_for_session(session)?;
        let total_bytes = session.cached_entries().iter().map(|entry| entry.size).sum::<u64>();
        let _ = tx.send(ArchiveTaskEvent::Progress(ArchiveProgress {
            operation: Some(ArchiveOperation::ExtractAll {
                target_dir: target_dir.to_path_buf(),
            }),
            total_entries: Some(session.cached_entries().len() as u64),
            total_bytes: Some(total_bytes),
            percent: Some(0.0),
            ..ArchiveProgress::default()
        }));
        backend.extract_all(session, target_dir)?;
        let _ = tx.send(ArchiveTaskEvent::Finished(format!(
            "Extracted archive to {} ({})",
            target_dir.display(),
            format_bytes(total_bytes)
        )));
        Ok(())
    }

    fn entries_for_archive_path(&self, session: &ArchiveSession, current_path: &str) -> Vec<Entry> {
        let prefix = archive_prefix(current_path);
        let mut entries_by_name = BTreeMap::<String, Entry>::new();
        let mut synthetic_dirs = BTreeSet::<String>::new();

        for archive_entry in session.cached_entries() {
            let Some(remainder) = archive_entry.archive_path.strip_prefix(&prefix) else {
                continue;
            };
            if remainder.is_empty() {
                continue;
            }

            if let Some((first, _rest)) = remainder.split_once('/') {
                synthetic_dirs.insert(first.to_string());
                continue;
            }

            entries_by_name.insert(
                archive_entry.display_name.clone(),
                archive_entry_to_panel_entry(archive_entry),
            );
        }

        for directory_name in synthetic_dirs {
            entries_by_name.entry(directory_name.clone()).or_insert_with(|| Entry {
                name: directory_name.clone(),
                archive_path: Some(join_archive_path(current_path, &directory_name)),
                is_dir: true,
                size_bytes: 0,
                size_label: "-".into(),
                type_label: "Folder".into(),
                modified_at: None,
                modified_label: "-".into(),
                attributes_label: "D".into(),
                is_parent_link: false,
            });
        }

        let mut entries = entries_by_name.into_values().collect::<Vec<_>>();
        entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        if session.cached_entries().iter().any(|_| true) {
            entries.insert(0, Entry::parent_link());
        }
        entries
    }
}

impl Default for ArchiveService {
    fn default() -> Self {
        Self::empty()
    }
}

fn archive_entry_to_panel_entry(entry: &ArchiveEntry) -> Entry {
    let is_dir = entry.kind == ArchiveEntryKind::Directory;
    Entry {
        name: entry.display_name.clone(),
        archive_path: Some(entry.archive_path.clone()),
        is_dir,
        size_bytes: entry.size,
        size_label: if is_dir {
            "-".into()
        } else {
            format_bytes(entry.size)
        },
        type_label: match entry.kind {
            ArchiveEntryKind::Directory => "Folder".into(),
            ArchiveEntryKind::File => "File".into(),
            ArchiveEntryKind::Symlink => "Symlink".into(),
            ArchiveEntryKind::Unknown => "Archive entry".into(),
        },
        modified_at: entry.modified_time,
        modified_label: entry
            .modified_time
            .map(|time| {
                let datetime: chrono::DateTime<chrono::Local> = time.into();
                datetime.format("%Y-%m-%d %H:%M").to_string()
            })
            .unwrap_or_else(|| "-".into()),
        attributes_label: entry.attributes.clone().unwrap_or_else(|| "-".into()),
        is_parent_link: false,
    }
}

fn archive_prefix(current_path: &str) -> String {
    if current_path.is_empty() {
        String::new()
    } else {
        format!("{current_path}/")
    }
}

fn join_archive_path(current_path: &str, child_name: &str) -> String {
    if current_path.is_empty() {
        child_name.into()
    } else {
        format!("{current_path}/{child_name}")
    }
}

fn validate_all_paths(target_dir: &Path, entries: &[ArchiveEntry]) -> Result<(), ArchiveError> {
    for entry in entries {
        safe_join_extract_path(target_dir, &entry.archive_path)?;
    }
    Ok(())
}

fn validate_selection_paths(
    target_dir: &Path,
    entries: &[ArchiveEntry],
    selection: &[String],
) -> Result<(), ArchiveError> {
    let selected = selection.iter().cloned().collect::<BTreeSet<_>>();
    for entry in entries {
        if selected.contains(&entry.archive_path)
            || selected
                .iter()
                .any(|prefix| entry.archive_path.starts_with(&format!("{prefix}/")))
        {
            safe_join_extract_path(target_dir, &entry.archive_path)?;
        }
    }
    Ok(())
}
