use std::{rc::Rc, sync::mpsc::TryRecvError, time::Duration};

use gtk::glib;
use rust_i18n::t;

use crate::ui::{
    dialogs,
    main_window::MainWindow,
    navigation::{self, LoadAction, NavigationRequest, SelectedNavigation},
};

impl MainWindow {
    pub(super) fn start_selected_navigation(
        self: &Rc<Self>,
        panel: crate::application::ActivePanel,
    ) {
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
                self.set_status_message(status);
            }
            SelectedNavigation::AskArchiveAction { path } => {
                let this = Rc::clone(self);
                dialogs::prompt_archive_open_action(
                    &self.window,
                    path.clone(),
                    move |open_as_archive| {
                        if open_as_archive {
                            let next_location = {
                                let archive_service = this.archive_service.borrow();
                                match archive_service.archive_location_for_path(&path) {
                                    Ok(location) => location,
                                    Err(error) => {
                                        this.show_command_failed(error);
                                        return;
                                    }
                                }
                            };

                            this.start_directory_load(NavigationRequest {
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

                        if let Err(error) = crate::platform::open_path(&path) {
                            this.show_command_failed(error);
                            return;
                        }

                        this.set_status_message(
                            t!(
                                "status.opened_with_default_app",
                                path = path.display().to_string()
                            )
                            .into_owned(),
                        );
                    },
                );
            }
            SelectedNavigation::Unsupported { message } => {
                self.show_command_failed(message);
            }
        }
    }

    pub(super) fn start_root_navigation(
        self: &Rc<Self>,
        panel: crate::application::ActivePanel,
        index: usize,
    ) {
        let request = {
            let commander = self.commander.borrow();
            navigation::root_navigation_request(&commander, panel, index)
        };

        let Some(request) = request else {
            return;
        };

        self.start_directory_load(request);
    }

    pub(super) fn start_directory_load(self: &Rc<Self>, request: NavigationRequest) {
        if self.navigation_busy.get() || self.active_operation.borrow().is_some() {
            return;
        }

        let request = self.prepare_navigation_request(request);
        self.set_navigation_busy(true, &request.busy_message);

        let archive_service = self.archive_service.borrow().clone();
        let show_hidden_files = self.app_config_cache.borrow().panels.show_hidden_files;
        let request_for_tracking = request.clone();
        let rx = navigation::spawn_directory_load(request, archive_service, show_hidden_files);

        let this = Rc::clone(self);
        glib::timeout_add_local(Duration::from_millis(30), move || match rx.try_recv() {
            Ok(result) => {
                this.set_navigation_busy(false, "");
                match result {
                    Ok(load) => {
                        if !this.commit_loaded_generation(load.panel, load.generation) {
                            return glib::ControlFlow::Break;
                        }
                        let update = {
                            let mut commander = this.commander.borrow_mut();
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
                        this.apply_update(update);
                        this.commander_view
                            .apply_root(this.commander.borrow().state(), load.panel);
                        this.focus_active_panel();
                        this.trigger_manual_refresh_cooldown();
                        this.refresh_dirty_panels_if_idle();
                    }
                    Err(error) => {
                        this.finish_in_flight_load(
                            request_for_tracking.panel,
                            request_for_tracking.generation,
                        );
                        let update = {
                            let mut commander = this.commander.borrow_mut();
                            commander.set_status(
                                t!("status.navigation_failed", error = error.as_str()).into_owned(),
                            )
                        };
                        this.apply_update(update);
                        this.focus_active_panel();
                        dialogs::show_error(
                            &this.window,
                            &t!("error.could_not_open_directory"),
                            &error,
                        );
                    }
                }
                glib::ControlFlow::Break
            }
            Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(TryRecvError::Disconnected) => {
                this.set_navigation_busy(false, "");
                this.finish_in_flight_load(
                    request_for_tracking.panel,
                    request_for_tracking.generation,
                );
                let update = {
                    let mut commander = this.commander.borrow_mut();
                    commander.set_status(t!("status.navigation_loader_disconnected").into_owned())
                };
                this.apply_update(update);
                this.focus_active_panel();
                glib::ControlFlow::Break
            }
        });
    }
}
