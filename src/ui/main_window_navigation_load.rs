use rust_i18n::t;

use crate::{
    application::{spawn_directory_load, LoadAction, NavigationError, NavigationRequest},
    remote::RemoteService,
    ui::{dialogs, glib_poll},
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
        let panel = request.panel;
        self.host
            .set_navigation_busy(panel, true, request.busy_message.as_str());

        let archive_service = self.archive_service.borrow().clone();
        let remote_service = self.remote_service.clone();
        let session_store = std::rc::Rc::clone(&self.session_store);
        let show_hidden_files = self.app_config_cache.borrow().panels.show_hidden_files;
        let request_for_tracking = request.clone();
        let request_for_disconnect = request_for_tracking.clone();
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
                controller
                    .host
                    .set_navigation_busy(request_for_tracking.panel, false, "");
                match result {
                    Ok(load) => {
                        if !controller.commit_loaded_generation(load.panel, load.generation) {
                            return false;
                        }
                        {
                            let commander = controller.commander.borrow();
                            if commander.state().active_tab_id(load.panel) != load.tab_id {
                                // Tab changed while loading; keep the other tab's in-memory state.
                                controller.host.notify_initial_panel_loaded(load.panel);
                                return false;
                            }
                        }
                        let update = {
                            let mut commander = controller.commander.borrow_mut();
                            match load.action {
                                LoadAction::Navigate => commander.navigate_to_loaded(
                                    load.panel,
                                    load.next_location,
                                    load.entries,
                                    load.status,
                                    load.selection_intent,
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
                        if let NavigationError::HostKeyTrustRequired { offer } = *error {
                            let retry_request = request_for_tracking.clone();
                            let trust_controller = controller.clone();
                            let window = trust_controller.window.clone();
                            let trusted_offer = offer.clone();
                            dialogs::confirm_ssh_host_key(&window, &offer, move || {
                                if let Err(error) = RemoteService::trust_host_key(&trusted_offer) {
                                    trust_controller.host.show_error(
                                        &t!("error.could_not_trust_host_key"),
                                        &error.to_string(),
                                    );
                                    trust_controller.host.focus_active_panel();
                                    return;
                                }
                                trust_controller.host.set_status(
                                    t!(
                                        "status.host_key_trusted",
                                        host = trusted_offer.host.as_str(),
                                        port = trusted_offer.port
                                    )
                                    .into_owned(),
                                );
                                trust_controller.start_directory_load(retry_request);
                            });
                            controller
                                .host
                                .notify_initial_panel_loaded(request_for_tracking.panel);
                            return false;
                        }

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
                disconnect_controller.host.set_navigation_busy(
                    request_for_disconnect.panel,
                    false,
                    "",
                );
                disconnect_controller.finish_in_flight_load(
                    request_for_disconnect.panel,
                    request_for_disconnect.generation,
                );
                let update = {
                    let mut commander = disconnect_controller.commander.borrow_mut();
                    commander.set_status(t!("status.navigation_loader_disconnected").into_owned())
                };
                disconnect_controller.host.apply_update(update);
                disconnect_controller.host.focus_active_panel();
                disconnect_controller
                    .host
                    .notify_initial_panel_loaded(request_for_disconnect.panel);
                false
            },
        );
    }

    pub(super) fn prepare_navigation_request(
        &self,
        mut request: NavigationRequest,
    ) -> NavigationRequest {
        {
            let mut commander = self.commander.borrow_mut();
            let panel = request.panel;
            request.tab_id = commander.state().active_tab_id(panel);
            request.folders_first = commander.state().panel(panel).folders_first;

            let mut sort_state = commander.state().panel(panel).sort_state;
            if let Some(path) = request.next_location.filesystem_path() {
                let folder_sort = &self.app_config_cache.borrow().folder_sort;
                let session_override = commander
                    .state()
                    .panel(panel)
                    .folder_sort_session_override(path);
                if let Some(resolved) = crate::domain::resolve_folder_sort_for_navigation(
                    folder_sort,
                    path,
                    session_override,
                ) {
                    sort_state = resolved;
                    commander.adopt_sort_state(panel, resolved);
                }
            }
            request.sort_state = sort_state;
        }
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
