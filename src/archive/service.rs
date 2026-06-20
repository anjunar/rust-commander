use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
};

use rust_i18n::t;

use super::{
    ArchiveBackendRegistry, ArchiveEntry, ArchiveEntryKind, ArchiveError, ArchiveOperation,
    ArchiveProbe, ArchiveProgress, ArchiveSession,
};
use crate::{
    application::TaskSpawner,
    archive::safe_join_extract_path,
    domain::{ArchiveView, Entry, EntryKind},
    fs::reader::format_bytes,
};

#[derive(Clone, Debug)]
pub struct ArchiveService {
    registry: Arc<ArchiveBackendRegistry>,
    task_spawner: TaskSpawner,
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
    pub fn new(registry: ArchiveBackendRegistry, task_spawner: TaskSpawner) -> Self {
        Self {
            registry: Arc::new(registry),
            task_spawner,
        }
    }

    pub fn with_default_backends(task_spawner: TaskSpawner) -> Self {
        Self::new(
            ArchiveBackendRegistry::with_default_backends(),
            task_spawner,
        )
    }

    pub fn is_archive_path(&self, path: &Path) -> bool {
        self.registry.is_archive_path(path)
    }

    pub fn probe_path(&self, path: &Path) -> Option<ArchiveProbe> {
        self.registry.probe_path(path)
    }

    pub fn open_archive(&self, path: &Path) -> Result<ArchiveSession, ArchiveError> {
        self.registry.resolve_for_path(path)?.open(path)
    }

    pub fn entries_for_archive_view(
        &self,
        view: &ArchiveView,
        session: &ArchiveSession,
    ) -> Vec<Entry> {
        self.entries_for_archive_path(session, &view.current_path)
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

        self.task_spawner.spawn(move || {
            let result = match request {
                ArchiveTaskRequest::ExtractSelection {
                    session,
                    entry_paths,
                    target_dir,
                } => {
                    service.extract_selection(&session, &entry_paths, &target_dir, &cancelled, &tx)
                }
                ArchiveTaskRequest::ExtractAll {
                    session,
                    target_dir,
                } => service.extract_all(&session, &target_dir, &cancelled, &tx),
                ArchiveTaskRequest::TestArchive { session } => {
                    match service.backend_for_session(&session) {
                        Ok(backend) => {
                            let result = backend.test_archive(&session);
                            if result.is_ok() {
                                let _ = tx.send(ArchiveTaskEvent::Finished(format!(
                                    "{}",
                                    t!(
                                        "archive.test_completed",
                                        path = session.archive_path().display().to_string()
                                    )
                                )));
                            }
                            result
                        }
                        Err(error) => Err(error),
                    }
                }
            };

            if cancelled.load(Ordering::Relaxed) {
                let _ = tx.send(ArchiveTaskEvent::Cancelled);
            } else if let Err(error) = result {
                let _ = tx.send(ArchiveTaskEvent::Failed(error));
            }
        });

        (handle, rx)
    }

    pub fn backend_for_session(
        &self,
        session: &ArchiveSession,
    ) -> Result<Arc<dyn super::ArchiveBackend>, ArchiveError> {
        self.registry.backend_for_session(session)
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
        let _ = tx.send(ArchiveTaskEvent::Progress(ArchiveProgress {
            operation: Some(ArchiveOperation::ExtractEntries {
                entry_paths: entry_paths.to_vec(),
                target_dir: target_dir.to_path_buf(),
            }),
            processed_entries: Some(0),
            total_entries: Some(total_entries),
            percent: Some(0.0),
            ..ArchiveProgress::default()
        }));

        backend.extract_entries(session, entry_paths, target_dir)?;
        let _ = tx.send(ArchiveTaskEvent::Finished(
            t!(
                "archive.extracted_items",
                count = entry_paths.len(),
                target = target_dir.display().to_string()
            )
            .into_owned(),
        ));
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
        let total_bytes = session
            .cached_entries()
            .iter()
            .map(|entry| entry.size)
            .sum::<u64>();
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
        let _ = tx.send(ArchiveTaskEvent::Finished(
            t!(
                "archive.extracted_archive",
                target = target_dir.display().to_string(),
                size = format_bytes(total_bytes)
            )
            .into_owned(),
        ));
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
            entries_by_name
                .entry(directory_name.clone())
                .or_insert_with(|| Entry {
                    name: directory_name.clone(),
                    archive_path: Some(join_archive_path(current_path, &directory_name)),
                    remote_path: None,
                    kind: EntryKind::Directory,
                    is_dir: true,
                    size_bytes: 0,
                    modified_at: None,
                    attributes: "D".into(),
                    is_parent_link: false,
                });
        }

        let mut entries = entries_by_name.into_values().collect::<Vec<_>>();
        entries.sort_by_key(|a| a.name.to_lowercase());
        if session.cached_entries().iter().any(|_| true) {
            entries.insert(0, Entry::parent_link());
        }
        entries
    }
}

impl Default for ArchiveService {
    fn default() -> Self {
        Self::with_default_backends(TaskSpawner::default())
    }
}

fn archive_entry_to_panel_entry(entry: &ArchiveEntry) -> Entry {
    let is_dir = entry.kind == ArchiveEntryKind::Directory;
    Entry {
        name: entry.display_name.clone(),
        archive_path: Some(entry.archive_path.clone()),
        remote_path: None,
        kind: archive_entry_kind(entry.kind),
        is_dir,
        size_bytes: entry.size,
        modified_at: entry.modified_time,
        attributes: entry.attributes.clone().unwrap_or_default(),
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

fn archive_entry_kind(kind: ArchiveEntryKind) -> EntryKind {
    match kind {
        ArchiveEntryKind::Directory => EntryKind::Directory,
        ArchiveEntryKind::File => EntryKind::File,
        ArchiveEntryKind::Symlink => EntryKind::Symlink,
        ArchiveEntryKind::Unknown => EntryKind::ArchiveItem,
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
