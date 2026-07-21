use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    sync::mpsc::{self, Sender},
    time::{Duration, Instant},
};

use rust_i18n::t;

use crate::{
    application::SharedPlatformPort,
    application::{
        parent_navigation_request, root_navigation_request, selected_navigation_request,
        ActivePanel, Commander, LoadAction, LoadScheduler, NavigationRequest, SelectedNavigation,
        SessionStore, TaskSpawner,
    },
    archive::{ArchiveProbe, ArchiveService, ArchiveSupport},
    config::AppConfig,
    fs::watcher::WatchCommand,
    remote::RemoteService,
};

use gtk::glib;

use super::{hosts::NavigationHost, operations_controller::OperationRuntime};

#[path = "main_window_navigation_load.rs"]
mod load;
#[path = "main_window_navigation_watch.rs"]
mod watch;

#[derive(Clone)]
pub struct NavigationRuntime {
    pub navigation_busy: Rc<Cell<bool>>,
    pub watcher_refresh_cooldown_until: Rc<Cell<Option<Instant>>>,
    pub load_scheduler: Rc<RefCell<LoadScheduler>>,
    pub watch_command_tx: Sender<WatchCommand>,
}

impl NavigationRuntime {
    pub fn new(watch_command_tx: Sender<WatchCommand>) -> Self {
        Self {
            navigation_busy: Rc::new(Cell::new(false)),
            watcher_refresh_cooldown_until: Rc::new(Cell::new(None)),
            load_scheduler: Rc::new(RefCell::new(LoadScheduler::default())),
            watch_command_tx,
        }
    }
}

#[derive(Clone)]
pub struct NavigationController {
    host: Rc<dyn NavigationHost>,
    window: gtk::ApplicationWindow,
    commander: Rc<RefCell<Commander>>,
    archive_service: Rc<RefCell<ArchiveService>>,
    remote_service: RemoteService,
    session_store: Rc<RefCell<SessionStore>>,
    task_spawner: TaskSpawner,
    operation_runtime: OperationRuntime,
    runtime: NavigationRuntime,
    app_config_cache: Rc<RefCell<AppConfig>>,
    platform_port: SharedPlatformPort,
}

pub struct NavigationControllerDeps {
    pub host: Rc<dyn NavigationHost>,
    pub window: gtk::ApplicationWindow,
    pub commander: Rc<RefCell<Commander>>,
    pub archive_service: Rc<RefCell<ArchiveService>>,
    pub remote_service: RemoteService,
    pub session_store: Rc<RefCell<SessionStore>>,
    pub task_spawner: TaskSpawner,
    pub operation_runtime: OperationRuntime,
    pub runtime: NavigationRuntime,
    pub app_config_cache: Rc<RefCell<AppConfig>>,
    pub platform_port: SharedPlatformPort,
}

impl NavigationController {
    pub fn new(deps: NavigationControllerDeps) -> Self {
        Self {
            host: deps.host,
            window: deps.window,
            commander: deps.commander,
            archive_service: deps.archive_service,
            remote_service: deps.remote_service,
            session_store: deps.session_store,
            task_spawner: deps.task_spawner,
            operation_runtime: deps.operation_runtime,
            runtime: deps.runtime,
            app_config_cache: deps.app_config_cache,
            platform_port: deps.platform_port,
        }
    }

    pub fn select_single_and_start(&self, panel: ActivePanel, index: usize) {
        let update = {
            let mut commander = self.commander.borrow_mut();
            commander.select_single(panel, index)
        };
        self.host.apply_update(update);
        self.start_selected_navigation(panel);
    }

    pub fn start_parent_navigation(&self, panel: ActivePanel) {
        let request = {
            let commander = self.commander.borrow();
            parent_navigation_request(&commander, panel)
        };
        let Some(request) = request else {
            return;
        };
        self.start_directory_load(request);
    }

    pub fn start_enter_selected_directory(&self, panel: ActivePanel) {
        let can_enter = {
            let commander = self.commander.borrow();
            commander
                .state()
                .panel(panel)
                .selected_item()
                .is_some_and(|entry| entry.is_dir || entry.is_parent_link)
        };
        if can_enter {
            self.start_selected_navigation(panel);
        }
    }

    pub fn start_selected_navigation(&self, panel: ActivePanel) {
        let request = {
            let commander = self.commander.borrow();
            let archive_service = self.archive_service.borrow();
            selected_navigation_request(&commander, &archive_service, panel)
        };

        match request {
            SelectedNavigation::Load(request) => self.start_directory_load(*request),
            SelectedNavigation::OpenPath { path, status } => {
                // Never shell-associate `.7z` in this process on Windows.
                #[cfg(target_os = "windows")]
                if crate::archive::ArchiveFormatDetector::is_seven_zip_path(&path) {
                    self.open_archive_location(panel, path);
                    return;
                }

                // Defer so the Enter key that triggered navigation cannot re-fire.
                let controller = self.clone();
                glib::idle_add_local_once(move || {
                    if let Err(error) = controller.platform_port.open_path(&path) {
                        controller.show_command_failed(error);
                        return;
                    }
                    controller.host.set_status(status);
                });
            }
            SelectedNavigation::OpenArchive { path } => {
                let can_open = self.archive_service.borrow().can_open_path(&path);
                if !can_open {
                    let unsupported_reason = self
                        .archive_service
                        .borrow()
                        .probe_path(&path)
                        .and_then(|probe: ArchiveProbe| match probe.support {
                            ArchiveSupport::NotSupportedYet { reason } => Some(reason),
                            ArchiveSupport::Supported => None,
                        });
                    if let Some(reason) = unsupported_reason {
                        let controller = self.clone();
                        glib::idle_add_local_once(move || {
                            controller.host.set_status(
                                t!(
                                    "status.archive_unsupported",
                                    path = path.display().to_string(),
                                    reason = reason.clone()
                                )
                                .into_owned(),
                            );
                            controller.host.show_error(
                                &t!("dialog.archive_unsupported_title"),
                                &t!(
                                    "dialog.archive_unsupported_detail",
                                    path = path.display().to_string(),
                                    reason = reason
                                ),
                            );
                        });
                        return;
                    }
                }

                let controller = self.clone();
                glib::idle_add_local_once(move || {
                    controller.open_archive_location(panel, path);
                });
            }
            SelectedNavigation::Unsupported { message } => self.show_command_failed(message),
        }
    }

}

fn panic_payload_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic".into()
    }
}

impl NavigationController {
    fn open_archive_location(&self, panel: ActivePanel, path: PathBuf) {
        self.host
            .set_status(t!("status.opening_archive").into_owned());

        let service = self.archive_service.borrow().clone();
        let path_for_worker = path.clone();
        let (tx, rx) = mpsc::channel();
        self.task_spawner.spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                service.open_archive(&path_for_worker)
            }));
            let mapped = match result {
                Ok(value) => value,
                Err(payload) => Err(crate::archive::ArchiveError::LibraryError {
                    library: "archive".into(),
                    detail: format!(
                        "Opening archive panicked: {} ({})",
                        path_for_worker.display(),
                        panic_payload_message(&payload)
                    ),
                }),
            };
            let _ = tx.send(mapped);
        });

        let controller = self.clone();
        crate::ui::glib_poll::poll_receiver(
            Duration::from_millis(20),
            rx,
            move |result| {
                match result {
                    Ok(session) => {
                        let session_key = controller
                            .session_store
                            .borrow_mut()
                            .insert_archive(session);
                        let next_location = crate::domain::PanelLocation::archive(
                            session_key,
                            path.clone(),
                            "",
                        );
                        controller.start_directory_load(NavigationRequest {
                            panel,
                            tab_id: 0,
                            generation: 0,
                            action: LoadAction::Navigate,
                            status: t!(
                                "status.opened_archive",
                                path = path.display().to_string()
                            )
                            .into_owned(),
                            next_location,
                            selection_intent: None,
                            busy_message: t!("status.opening_archive").into_owned(),
                            sort_state: Default::default(),
                            folders_first: true,
                        });
                    }
                    Err(error) => controller.show_command_failed(error),
                }
                false
            },
            || false,
        );
    }

    pub fn start_root_navigation(&self, panel: ActivePanel, index: usize) {
        let request = {
            let commander = self.commander.borrow();
            root_navigation_request(&commander, panel, index)
        };

        let Some(request) = request else {
            return;
        };

        self.start_directory_load(request);
    }

    pub fn start_remote_session(&self, panel: ActivePanel, session: crate::remote::RemoteSession) {
        let profile = session.profile().clone();
        let start_directory = session.start_directory().to_string();
        let session_key = self.session_store.borrow_mut().insert_remote(session);
        self.start_directory_load(NavigationRequest {
            panel,
                tab_id: 0,
            generation: 0,
            action: LoadAction::Navigate,
            status: t!("status.opened", path = start_directory.as_str()).into_owned(),
            next_location: crate::domain::PanelLocation::remote(
                session_key,
                profile.auth.username(),
                profile.host,
                profile.port,
                start_directory,
            ),
            selection_intent: None,
            busy_message: "Connecting to remote host...".into(),
            sort_state: Default::default(),
            folders_first: true,
        });
    }

    pub fn sync_watched_paths(&self) {
        let paths = self.commander.borrow().state().visible_paths();
        let _ = self
            .runtime
            .watch_command_tx
            .send(WatchCommand::SetPaths(paths));
    }

    /// Space: toggle mark in place (no cursor move); when marking a folder, measure size in the background.
    pub fn handle_space_action(&self, panel: ActivePanel) {
        let dir_job = {
            let commander = self.commander.borrow();
            let panel_state = commander.state().panel(panel);
            let Some(entry) = panel_state.selected_entry() else {
                return;
            };
            let Some(index) = panel_state.cursor_index() else {
                return;
            };
            // Only recalculate when marking; unmarking must not remeasure.
            if panel_state.selection.is_marked(index) {
                None
            } else if !entry.is_dir || entry.is_parent_link {
                None
            } else {
                panel_state
                    .location
                    .entry_filesystem_path(entry)
                    .map(|path| {
                        (
                            crate::fs::reader::resolve_directory_navigation_path(&path),
                            entry.key(),
                            index,
                        )
                    })
            }
        };

        {
            let mut commander = self.commander.borrow_mut();
            // Ignore the heavy ViewUpdate::selection — marks are applied in-place below.
            let _ = commander.toggle_mark_in_place(panel);
        }
        self.host.apply_panel_selection(panel);

        let Some((path, key, index)) = dir_job else {
            return;
        };

        self.host.update_panel_row_size(panel, index, "...");

        let (tx, rx) = std::sync::mpsc::channel();
        self.task_spawner.spawn(move || {
            let bytes = crate::fs::reader::directory_total_size(&path);
            let _ = tx.send(bytes);
        });

        let controller = self.clone();
        crate::ui::glib_poll::poll_receiver(
            std::time::Duration::from_millis(50),
            rx,
            move |bytes| {
                let row_index = {
                    let mut commander = controller.commander.borrow_mut();
                    commander
                        .set_calculated_dir_size(panel, key.clone(), bytes)
                        .1
                };
                if let Some(row_index) = row_index {
                    let label = {
                        let commander = controller.commander.borrow();
                        let panel_state = commander.state().panel(panel);
                        panel_state
                            .entries
                            .get(row_index)
                            .map(|entry| panel_state.size_label_for(entry))
                            .unwrap_or_else(|| crate::fs::reader::format_bytes(bytes))
                    };
                    controller
                        .host
                        .update_panel_row_size(panel, row_index, &label);
                }
                false
            },
            || false,
        );
    }
}
