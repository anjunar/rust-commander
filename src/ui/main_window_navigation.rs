use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    sync::mpsc::{Receiver, Sender, TryRecvError},
    time::{Duration, Instant},
};

use gtk::glib;
use rust_i18n::t;

use crate::{
    application::{ActivePanel, Commander, LoadScheduler},
    archive::ArchiveService,
    config::AppConfig,
    fs::watcher::{WatchCommand, WatchEvent},
    ui::{
        dialogs,
        navigation::{self, LoadAction, NavigationRequest, SelectedNavigation},
    },
};

use super::{hosts::NavigationHost, operations_controller::OperationRuntime};

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
    operation_runtime: OperationRuntime,
    runtime: NavigationRuntime,
    app_config_cache: Rc<RefCell<AppConfig>>,
}

impl NavigationController {
    pub fn new(
        host: Rc<dyn NavigationHost>,
        window: gtk::ApplicationWindow,
        commander: Rc<RefCell<Commander>>,
        archive_service: Rc<RefCell<ArchiveService>>,
        operation_runtime: OperationRuntime,
        runtime: NavigationRuntime,
        app_config_cache: Rc<RefCell<AppConfig>>,
    ) -> Self {
        Self {
            host,
            window,
            commander,
            archive_service,
            operation_runtime,
            runtime,
            app_config_cache,
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
            navigation::selected_navigation_request(&commander, &archive_service, panel)
        };

        match request {
            SelectedNavigation::Load(request) => self.start_directory_load(request),
            SelectedNavigation::OpenPath { path, status } => {
                if let Err(error) = crate::platform::open_path(&path) {
                    self.show_command_failed(error);
                    return;
                }
                self.host.set_status(status);
            }
            SelectedNavigation::AskArchiveAction { path } => {
                let controller = self.clone();
                dialogs::prompt_archive_open_action(&self.window, path.clone(), move |open_as_archive| {
                    if open_as_archive {
                        let next_location = {
                            let archive_service = controller.archive_service.borrow();
                            match archive_service.archive_location_for_path(&path) {
                                Ok(location) => location,
                                Err(error) => {
                                    controller.show_command_failed(error);
                                    return;
                                }
                            }
                        };

                        controller.start_directory_load(NavigationRequest {
                            panel,
                            generation: 0,
                            action: LoadAction::Navigate,
                            status: t!("status.opened_archive", path = path.display().to_string())
                                .into_owned(),
                            next_location,
                            selection_intent: None,
                            busy_message: t!("status.opening_archive").into_owned(),
                        });
                        return;
                    }

                    if let Err(error) = crate::platform::open_path(&path) {
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
                });
            }
            SelectedNavigation::Unsupported { message } => self.show_command_failed(message),
        }
    }

    pub fn start_root_navigation(&self, panel: ActivePanel, index: usize) {
        let request = {
            let commander = self.commander.borrow();
            navigation::root_navigation_request(&commander, panel, index)
        };

        let Some(request) = request else {
            return;
        };

        self.start_directory_load(request);
    }

    pub fn start_directory_load(&self, request: NavigationRequest) {
        if self.runtime.navigation_busy.get()
            || self.operation_runtime.active_operation.borrow().is_some()
        {
            return;
        }

        let request = self.prepare_navigation_request(request);
        self.host
            .set_navigation_busy(true, request.busy_message.as_str());

        let archive_service = self.archive_service.borrow().clone();
        let show_hidden_files = self.app_config_cache.borrow().panels.show_hidden_files;
        let request_for_tracking = request.clone();
        let rx = navigation::spawn_directory_load(request, archive_service, show_hidden_files);

        let controller = self.clone();
        glib::timeout_add_local(Duration::from_millis(30), move || match rx.try_recv() {
            Ok(result) => {
                controller.host.set_navigation_busy(false, "");
                match result {
                    Ok(load) => {
                        if !controller.commit_loaded_generation(load.panel, load.generation) {
                            return glib::ControlFlow::Break;
                        }
                        let update = {
                            let mut commander = controller.commander.borrow_mut();
                            match load.action {
                                LoadAction::Navigate => commander.navigate_to_loaded(
                                    load.panel,
                                    load.next_location,
                                    load.entries,
                                    load.status,
                                ),
                                LoadAction::Refresh => commander.refresh_panel_loaded(
                                    load.panel,
                                    load.entries,
                                    load.status,
                                    load.selection_intent,
                                ),
                            }
                        };
                        controller.host.apply_update(update);
                        controller.host.apply_panel_root(load.panel);
                        controller.host.focus_active_panel();
                        controller.host.notify_initial_panel_loaded(load.panel);
                        controller.trigger_manual_refresh_cooldown();
                        controller.refresh_dirty_panels_if_idle();
                    }
                    Err(error) => {
                        controller.finish_in_flight_load(
                            request_for_tracking.panel,
                            request_for_tracking.generation,
                        );
                        let update = {
                            let mut commander = controller.commander.borrow_mut();
                            commander.set_status(
                                t!("status.navigation_failed", error = error.as_str()).into_owned(),
                            )
                        };
                        controller.host.apply_update(update);
                        controller.host.focus_active_panel();
                        controller
                            .host
                            .notify_initial_panel_loaded(request_for_tracking.panel);
                        controller
                            .host
                            .show_error(&t!("error.could_not_open_directory"), &error);
                    }
                }
                glib::ControlFlow::Break
            }
            Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(TryRecvError::Disconnected) => {
                controller.host.set_navigation_busy(false, "");
                controller.finish_in_flight_load(
                    request_for_tracking.panel,
                    request_for_tracking.generation,
                );
                let update = {
                    let mut commander = controller.commander.borrow_mut();
                    commander.set_status(t!("status.navigation_loader_disconnected").into_owned())
                };
                controller.host.apply_update(update);
                controller.host.focus_active_panel();
                controller
                    .host
                    .notify_initial_panel_loaded(request_for_tracking.panel);
                glib::ControlFlow::Break
            }
        });
    }

    pub fn mark_panels_dirty(&self, panels: &[ActivePanel]) {
        self.runtime
            .load_scheduler
            .borrow_mut()
            .queue_refresh(panels, t!("status.view_refreshed").into_owned());
    }

    pub fn refresh_dirty_panels_if_idle(&self) {
        if self.operation_runtime.active_operation.borrow().is_some()
            || self.runtime.navigation_busy.get()
            || self.is_watcher_refresh_suppressed()
        {
            return;
        }

        let Some((panel, status)) = self
            .runtime
            .load_scheduler
            .borrow_mut()
            .take_next_refresh(&t!("status.view_refreshed").into_owned())
        else {
            return;
        };
        let request = {
            let commander = self.commander.borrow();
            navigation::refresh_request(&commander, panel, status)
        };
        self.start_directory_load(request);
    }

    pub fn queue_initial_panel_loads(&self) {
        self.runtime.load_scheduler.borrow_mut().queue_refresh(
            &[ActivePanel::Left, ActivePanel::Right],
            t!("status.view_refreshed").into_owned(),
        );
        self.refresh_dirty_panels_if_idle();
    }

    pub fn sync_watched_paths(&self) {
        let paths = self.commander.borrow().state().visible_paths();
        let _ = self.runtime.watch_command_tx.send(WatchCommand::SetPaths(paths));
    }

    pub fn install_watcher_poll(&self, watch_event_rx: Receiver<WatchEvent>) {
        let controller = self.clone();
        glib::timeout_add_local(Duration::from_millis(350), move || {
            let mut changed_paths = Vec::new();
            while let Ok(event) = watch_event_rx.try_recv() {
                changed_paths.extend(event.paths);
            }

            if !changed_paths.is_empty() {
                let affected_panels = controller.affected_panels_for_paths(&changed_paths);
                controller.mark_panels_dirty(&affected_panels);
            }

            controller.refresh_dirty_panels_if_idle();

            glib::ControlFlow::Continue
        });
    }

    pub fn affected_panels_for_paths(&self, changed_paths: &[PathBuf]) -> Vec<ActivePanel> {
        let commander = self.commander.borrow();
        let state = commander.state();
        let mut affected = Vec::new();

        for panel in [ActivePanel::Left, ActivePanel::Right] {
            let Some(panel_path) = state.panel(panel).location.filesystem_path() else {
                continue;
            };
            if changed_paths
                .iter()
                .any(|path| path == panel_path || path.parent() == Some(panel_path))
            {
                affected.push(panel);
            }
        }

        affected
    }

    fn prepare_navigation_request(&self, request: NavigationRequest) -> NavigationRequest {
        self.runtime.load_scheduler.borrow_mut().prepare_request(request)
    }

    fn commit_loaded_generation(&self, panel: ActivePanel, generation: u64) -> bool {
        self.runtime
            .load_scheduler
            .borrow_mut()
            .commit_loaded(panel, generation)
    }

    fn finish_in_flight_load(&self, panel: ActivePanel, generation: u64) {
        self.runtime
            .load_scheduler
            .borrow_mut()
            .finish_in_flight(panel, generation);
    }

    fn trigger_manual_refresh_cooldown(&self) {
        self.runtime
            .watcher_refresh_cooldown_until
            .set(Some(Instant::now() + Duration::from_millis(900)));
        self.sync_watched_paths();
    }

    fn is_watcher_refresh_suppressed(&self) -> bool {
        match self.runtime.watcher_refresh_cooldown_until.get() {
            Some(until) if Instant::now() < until => true,
            Some(_) => {
                self.runtime.watcher_refresh_cooldown_until.set(None);
                false
            }
            None => false,
        }
    }

    fn show_command_failed(&self, error: impl std::fmt::Display) {
        let error = error.to_string();
        self.host
            .set_status(t!("status.command_failed", error = error.as_str()).into_owned());
        self.host.show_error(&t!("error.command_failed"), &error);
    }
}
