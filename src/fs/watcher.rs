use std::{path::PathBuf, sync::mpsc, thread};

use notify::{RecursiveMode, Watcher};

pub enum WatchCommand {
    SetPaths(Vec<PathBuf>),
}

#[derive(Clone, Debug)]
pub struct WatchEvent {
    pub paths: Vec<PathBuf>,
}

pub fn start_file_watcher() -> (mpsc::Sender<WatchCommand>, mpsc::Receiver<WatchEvent>) {
    let (watch_command_tx, watch_command_rx) = mpsc::channel::<WatchCommand>();
    let (watch_event_tx, watch_event_rx) = mpsc::channel::<WatchEvent>();

    thread::spawn(move || {
        let callback_tx = watch_event_tx.clone();
        let watcher_result = notify::recommended_watcher(
            move |result: notify::Result<notify::Event>| match result {
                Ok(event) => {
                    let paths = dedupe_paths(event.paths);
                    if !paths.is_empty() {
                        let _ = callback_tx.send(WatchEvent { paths });
                    }
                }
                Err(error) => {
                    eprintln!("File watcher error: {error}");
                }
            },
        );

        let Ok(mut watcher) = watcher_result else {
            if let Err(error) = watcher_result {
                eprintln!("File watcher could not start: {error}");
            }
            return;
        };

        let mut watched_paths: Vec<PathBuf> = Vec::new();

        while let Ok(command) = watch_command_rx.recv() {
            match command {
                WatchCommand::SetPaths(paths) => {
                    let unique_paths = dedupe_paths(paths);

                    for old_path in &watched_paths {
                        if !unique_paths.iter().any(|path| path == old_path) {
                            let _ = watcher.unwatch(old_path);
                        }
                    }

                    for new_path in &unique_paths {
                        if !watched_paths.iter().any(|path| path == new_path) && new_path.is_dir() {
                            if let Err(error) =
                                watcher.watch(new_path, RecursiveMode::NonRecursive)
                            {
                                eprintln!(
                                    "Could not watch directory ({}): {error}",
                                    new_path.display()
                                );
                            }
                        }
                    }

                    watched_paths = unique_paths;
                }
            }
        }
    });

    (watch_command_tx, watch_event_rx)
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut unique_paths = Vec::new();

    for path in paths {
        if !unique_paths.iter().any(|existing| existing == &path) {
            unique_paths.push(path);
        }
    }

    unique_paths
}
