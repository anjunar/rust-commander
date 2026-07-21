use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use crate::domain::search::{compile_search, SearchHit, SearchOptions};

use super::reader::is_hidden;

const BATCH_SIZE: usize = 32;
const BATCH_INTERVAL: Duration = Duration::from_millis(100);
const PROGRESS_INTERVAL: Duration = Duration::from_millis(80);

/// Events streamed from a background find-files walk.
#[derive(Debug)]
pub enum SearchEvent {
    Progress {
        path: PathBuf,
        found: usize,
    },
    Hits(Vec<SearchHit>),
    Failed {
        message: String,
    },
    Finished {
        found: usize,
        errors: usize,
        cancelled: bool,
    },
}

/// Spawn a cancellable local filesystem search on a worker thread.
pub fn spawn_search(options: SearchOptions, cancel: Arc<AtomicBool>) -> Receiver<SearchEvent> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = run_search(options, cancel, &tx);
    });
    rx
}

fn run_search(
    options: SearchOptions,
    cancel: Arc<AtomicBool>,
    tx: &mpsc::Sender<SearchEvent>,
) -> Result<(), ()> {
    let compiled = match compile_search(&options) {
        Ok(compiled) => compiled,
        Err(message) => {
            let _ = tx.send(SearchEvent::Failed { message });
            return Ok(());
        }
    };

    let mut stack = vec![options.root.clone()];
    let mut found = 0usize;
    let mut errors = 0usize;
    let mut pending = Vec::new();
    let mut last_batch = Instant::now();
    let mut last_progress = Instant::now();

    let flush = |pending: &mut Vec<SearchHit>, tx: &mpsc::Sender<SearchEvent>| -> bool {
        if pending.is_empty() {
            return true;
        }
        let batch = std::mem::take(pending);
        tx.send(SearchEvent::Hits(batch)).is_ok()
    };

    while let Some(dir) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            let _ = flush(&mut pending, tx);
            let _ = tx.send(SearchEvent::Finished {
                found,
                errors,
                cancelled: true,
            });
            return Ok(());
        }

        if last_progress.elapsed() >= PROGRESS_INTERVAL {
            if tx
                .send(SearchEvent::Progress {
                    path: dir.clone(),
                    found,
                })
                .is_err()
            {
                return Ok(());
            }
            last_progress = Instant::now();
        }

        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => {
                errors += 1;
                continue;
            }
        };

        let mut children = Vec::new();
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    errors += 1;
                    continue;
                }
            };
            children.push(entry);
        }

        for entry in children {
            if cancel.load(Ordering::Relaxed) {
                let _ = flush(&mut pending, tx);
                let _ = tx.send(SearchEvent::Finished {
                    found,
                    errors,
                    cancelled: true,
                });
                return Ok(());
            }

            let path = entry.path();
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(_) => {
                    errors += 1;
                    continue;
                }
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            if !options.show_hidden_files && is_hidden(&metadata, &name) {
                continue;
            }

            if compiled.matches_exclude(&name) {
                continue;
            }

            let is_symlink = metadata.file_type().is_symlink();
            let is_dir = metadata.is_dir() || (is_symlink && path.is_dir());

            if compiled.matches_include(&name) {
                let size_bytes = if is_dir { 0 } else { metadata.len() };
                pending.push(SearchHit {
                    path: path.clone(),
                    name: name.clone(),
                    is_dir,
                    size_bytes,
                    modified_at: metadata.modified().ok(),
                });
                found += 1;
                if pending.len() >= BATCH_SIZE || last_batch.elapsed() >= BATCH_INTERVAL {
                    if !flush(&mut pending, tx) {
                        return Ok(());
                    }
                    last_batch = Instant::now();
                }
            }

            // Descend into real directories only (skip symlink dirs to avoid loops).
            if options.include_subfolders && metadata.is_dir() && !is_symlink {
                stack.push(path);
            }
        }
    }

    let _ = flush(&mut pending, tx);
    let _ = tx.send(SearchEvent::Finished {
        found,
        errors,
        cancelled: false,
    });
    Ok(())
}
