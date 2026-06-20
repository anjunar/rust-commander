use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::mpsc::Sender,
    time::Instant,
};

use rust_i18n::t;

use crate::{
    application::SharedPlatformPort,
    application::{
        root_navigation_request, selected_navigation_request, ActivePanel, Commander, LoadAction,
        LoadScheduler, NavigationRequest, SelectedNavigation, SessionStore, TaskSpawner,
    },
    archive::ArchiveService,
    config::AppConfig,
    fs::watcher::WatchCommand,
    remote::RemoteService,
    ui::dialogs,
};

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

    pub fn start_selected_navigation(&self, panel: ActivePanel) {
        let request = {
            let commander = self.commander.borrow();
            let archive_service = self.archive_service.borrow();
            selected_navigation_request(&commander, &archive_service, panel)
        };

        match request {
            SelectedNavigation::Load(request) => self.start_directory_load(*request),
            SelectedNavigation::OpenPath { path, status } => {
                if let Err(error) = self.platform_port.open_path(&path) {
                    self.show_command_failed(error);
                    return;
                }
                self.host.set_status(status);
            }
            SelectedNavigation::AskArchiveAction { path } => {
                let controller = self.clone();
                dialogs::prompt_archive_open_action(
                    &self.window,
                    path.clone(),
                    move |open_as_archive| {
                        if open_as_archive {
                            let next_location = {
                                let session = {
                                    let archive_service = controller.archive_service.borrow();
                                    match archive_service.open_archive(&path) {
                                        Ok(session) => session,
                                        Err(error) => {
                                            controller.show_command_failed(error);
                                            return;
                                        }
                                    }
                                };
                                let session_key = controller
                                    .session_store
                                    .borrow_mut()
                                    .insert_archive(session);
                                crate::domain::PanelLocation::archive(session_key, path.clone(), "")
                            };

                            controller.start_directory_load(NavigationRequest {
                                panel,
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
                            });
                            return;
                        }

                        if let Err(error) = controller.platform_port.open_path(&path) {
                            controller.show_command_failed(error);
                            return;
                        }

                        controller.host.set_status(
                            t!(
                                "status.opened_with_default_app",
                                path = path.display().to_string()
                            )
                            .into_owned(),
                        );
                    },
                );
            }
            SelectedNavigation::Unsupported { message } => self.show_command_failed(message),
        }
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
        });
    }

    pub fn sync_watched_paths(&self) {
        let paths = self.commander.borrow().state().visible_paths();
        let _ = self
            .runtime
            .watch_command_tx
            .send(WatchCommand::SetPaths(paths));
    }
}
