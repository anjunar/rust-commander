use rust_i18n::t;

use crate::{
    application::{spawn_directory_load, LoadAction, NavigationRequest},
    ui::glib_poll,
};

use super::NavigationController;

impl NavigationController {
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
        let remote_service = self.remote_service.clone();
        let session_store = std::rc::Rc::clone(&self.session_store);
        let show_hidden_files = self.app_config_cache.borrow().panels.show_hidden_files;
        let request_for_tracking = request.clone();
        let rx = spawn_directory_load(
            self.task_spawner.clone(),
            request,
            archive_service,
            remote_service,
            session_store,
            show_hidden_files,
        );

        let controller = self.clone();
        let disconnect_controller = controller.clone();
        glib_poll::poll_receiver(
            std::time::Duration::from_millis(30),
            rx,
            move |result| {
                controller.host.set_navigation_busy(false, "");
                match result {
                    Ok(load) => {
                        if !controller.commit_loaded_generation(load.panel, load.generation) {
                            return false;
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
                        controller.refresh_dirty_panels_if_idle();
                        controller.trigger_manual_refresh_cooldown();
                    }
                    Err(error) => {
                        controller.finish_in_flight_load(
                            request_for_tracking.panel,
                            request_for_tracking.generation,
                        );
                        let error_detail = error.detail().to_string();
                        let update = {
                            let mut commander = controller.commander.borrow_mut();
                            commander.set_status(
                                t!("status.navigation_failed", error = error_detail.as_str())
                                    .into_owned(),
                            )
                        };
                        controller.host.apply_update(update);
                        controller.host.focus_active_panel();
                        controller
                            .host
                            .notify_initial_panel_loaded(request_for_tracking.panel);
                        controller
                            .host
                            .show_error(&t!("error.could_not_open_directory"), &error_detail);
                    }
                }
                false
            },
            move || {
                disconnect_controller.host.set_navigation_busy(false, "");
                disconnect_controller.finish_in_flight_load(
                    request_for_tracking.panel,
                    request_for_tracking.generation,
                );
                let update = {
                    let mut commander = disconnect_controller.commander.borrow_mut();
                    commander.set_status(t!("status.navigation_loader_disconnected").into_owned())
                };
                disconnect_controller.host.apply_update(update);
                disconnect_controller.host.focus_active_panel();
                disconnect_controller
                    .host
                    .notify_initial_panel_loaded(request_for_tracking.panel);
                false
            },
        );
    }

    pub(super) fn prepare_navigation_request(
        &self,
        request: NavigationRequest,
    ) -> NavigationRequest {
        self.runtime
            .load_scheduler
            .borrow_mut()
            .prepare_request(request)
    }

    pub(super) fn commit_loaded_generation(
        &self,
        panel: crate::application::ActivePanel,
        generation: u64,
    ) -> bool {
        self.runtime
            .load_scheduler
            .borrow_mut()
            .commit_loaded(panel, generation)
    }

    pub(super) fn finish_in_flight_load(
        &self,
        panel: crate::application::ActivePanel,
        generation: u64,
    ) {
        self.runtime
            .load_scheduler
            .borrow_mut()
            .finish_in_flight(panel, generation);
    }

    pub(super) fn show_command_failed(&self, error: impl std::fmt::Display) {
        let error = error.to_string();
        self.host
            .set_status(t!("status.command_failed", error = error.as_str()).into_owned());
        self.host.show_error(&t!("error.command_failed"), &error);
    }
}
