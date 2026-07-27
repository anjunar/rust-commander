use std::rc::Rc;

use gtk::{gio, glib, prelude::*};

use crate::{
    application::{
        filesystem_navigation_request, location_root_navigation_request, ActivePanel,
        FileOperationKind,
    },
    domain::sorting::{SortColumn, SortDirection},
    presentation,
    ui::{dialogs, main_window::MainWindow},
};
use rust_i18n::t;

#[path = "main_window_actions_file.rs"]
mod file_actions;
#[path = "main_window_actions_remote.rs"]
mod remote_actions;
#[path = "main_window_actions_terminal.rs"]
mod terminal_actions;

impl MainWindow {
    pub fn handle_connect_remote_for_panel(
        self: &Rc<Self>,
        panel: crate::application::ActivePanel,
    ) {
        let update = {
            let mut commander = self.commander.borrow_mut();
            commander.set_active_panel(panel)
        };
        self.apply_update(update);
        self.handle_connect_remote();
    }

    pub fn handle_focus_active_panel(self: &Rc<Self>) {
        self.focus_active_panel();
    }

    pub fn handle_refresh(self: &Rc<Self>) {
        let navigation = self.navigation_controller();
        navigation.trigger_manual_refresh_cooldown();
        let request = {
            let commander = self.commander.borrow();
            let panel = commander.state().active_panel;
            crate::application::refresh_request(
                &commander,
                panel,
                t!("status.directory_refreshed").into_owned(),
            )
        };
        navigation.start_directory_load(request);
    }

    pub fn show_hidden_files_enabled(&self) -> bool {
        self.app_config_cache.borrow().panels.show_hidden_files
    }

    pub fn set_toggle_hidden_files_state(&self, enabled: bool) {
        let Some(action) = self.window.lookup_action("toggle-hidden-files") else {
            return;
        };
        let Ok(action) = action.downcast::<gio::SimpleAction>() else {
            return;
        };
        action.set_state(&enabled.to_variant());
    }

    pub fn handle_toggle_hidden_files(self: &Rc<Self>) {
        let show_hidden = match self.config_store.update(|config| {
            config.panels.show_hidden_files = !config.panels.show_hidden_files;
        }) {
            Ok(config) => config.panels.show_hidden_files,
            Err(error) => {
                dialogs::show_error(
                    &self.window,
                    &t!("error.could_not_save_settings"),
                    &error.to_string(),
                );
                return;
            }
        };
        self.set_toggle_hidden_files_state(show_hidden);

        let status = if show_hidden {
            t!("status.hidden_files_shown").into_owned()
        } else {
            t!("status.hidden_files_hidden").into_owned()
        };
        let panels = self.app_config_cache.borrow().panels.clone();
        let update = {
            let mut commander = self.commander.borrow_mut();
            match commander.apply_panel_settings(panels) {
                Ok(mut update) => {
                    commander.set_status(status.clone());
                    update.status = true;
                    update
                }
                Err(error) => {
                    dialogs::show_error(
                        &self.window,
                        &t!("error.could_not_save_settings"),
                        &error.to_string(),
                    );
                    return;
                }
            }
        };
        self.apply_update(update);
        self.navigation_controller()
            .refresh_panels(&[ActivePanel::Left, ActivePanel::Right], status);
    }

    pub fn handle_sort_by_name(self: &Rc<Self>) {
        self.sort_active_panel(SortColumn::Name);
    }

    pub fn handle_sort_by_extension(self: &Rc<Self>) {
        self.sort_active_panel(SortColumn::Type);
    }

    pub fn handle_sort_by_date(self: &Rc<Self>) {
        self.sort_active_panel(SortColumn::Modified);
    }

    pub fn handle_sort_by_size(self: &Rc<Self>) {
        self.sort_active_panel(SortColumn::Size);
    }

    fn sort_active_panel(self: &Rc<Self>, column: SortColumn) {
        let panel = self.commander.borrow().state().active_panel;
        let direction = {
            let current = self.commander.borrow().state().panel(panel).sort_state;
            if current.column == column {
                match current.direction {
                    SortDirection::Ascending => SortDirection::Descending,
                    SortDirection::Descending => SortDirection::Ascending,
                }
            } else {
                SortDirection::Ascending
            }
        };

        self.commander_view
            .panel(panel)
            .apply_sort(column, direction);

        let status = t!(
            "status.sorted_panel",
            panel = presentation::panel_label(panel),
            column = presentation::sort_column_label(column)
        )
        .into_owned();
        let update = {
            let mut commander = self.commander.borrow_mut();
            commander.sort_panel(panel, column, direction, status)
        };
        self.apply_update(update);
        self.focus_active_panel();
        self.window_state_controller().persist_pane_session();
    }

    pub fn handle_remember_folder_sort(self: &Rc<Self>) {
        let (path, sort_state) = {
            let commander = self.commander.borrow();
            let panel = commander.state().active_panel;
            let panel_state = commander.state().panel(panel);
            let Some(path) = panel_state
                .location
                .filesystem_path()
                .map(std::path::PathBuf::from)
            else {
                dialogs::show_error(
                    &self.window,
                    &t!("error.folder_sort_unavailable"),
                    &t!("error.folder_sort_filesystem_only"),
                );
                return;
            };
            (path, panel_state.sort_state)
        };

        if let Err(error) = self.config_store.update(|config| {
            crate::domain::upsert_folder_sort(&mut config.folder_sort, path.clone(), sort_state);
        }) {
            dialogs::show_error(
                &self.window,
                &t!("error.could_not_save_settings"),
                &error.to_string(),
            );
            return;
        }

        {
            let panel = self.commander.borrow().state().active_panel;
            self.commander
                .borrow_mut()
                .clear_folder_sort_session_override(panel, &path);
        }

        self.set_status_message(
            t!(
                "status.folder_sort_remembered",
                path = path.display().to_string()
            )
            .into_owned(),
        );
    }

    pub fn handle_forget_folder_sort(self: &Rc<Self>) {
        let path = {
            let commander = self.commander.borrow();
            let panel = commander.state().active_panel;
            let Some(path) = commander
                .state()
                .panel(panel)
                .location
                .filesystem_path()
                .map(std::path::PathBuf::from)
            else {
                dialogs::show_error(
                    &self.window,
                    &t!("error.folder_sort_unavailable"),
                    &t!("error.folder_sort_filesystem_only"),
                );
                return;
            };
            path
        };

        let mut removed = false;
        if let Err(error) = self.config_store.update(|config| {
            removed = crate::domain::remove_folder_sort(&mut config.folder_sort, &path);
        }) {
            dialogs::show_error(
                &self.window,
                &t!("error.could_not_save_settings"),
                &error.to_string(),
            );
            return;
        }

        let status = if removed {
            t!(
                "status.folder_sort_forgotten",
                path = path.display().to_string()
            )
            .into_owned()
        } else {
            t!(
                "status.folder_sort_not_set",
                path = path.display().to_string()
            )
            .into_owned()
        };
        self.set_status_message(status);
    }

    pub fn handle_select_root_left(self: &Rc<Self>) {
        self.open_root_selector_for(ActivePanel::Left);
    }

    pub fn handle_select_root_right(self: &Rc<Self>) {
        self.open_root_selector_for(ActivePanel::Right);
    }

    fn open_root_selector_for(self: &Rc<Self>, panel: ActivePanel) {
        self.set_command_bar_label_mode(super::command_bar::CommandBarLabelMode::Normal);
        let update = {
            let mut commander = self.commander.borrow_mut();
            commander.set_active_panel(panel)
        };
        self.apply_update(update);
        self.commander_view.panel(panel).open_root_selector();
    }

    pub fn handle_switch_panel(self: &Rc<Self>) {
        let update = {
            let mut commander = self.commander.borrow_mut();
            commander.switch_panel()
        };
        self.apply_update(update);
    }

    pub fn handle_swap_panels(self: &Rc<Self>) {
        let update = {
            let mut commander = self.commander.borrow_mut();
            commander.swap_panels()
        };
        self.apply_update(update);
        self.navigation_controller().sync_watched_paths();
    }

    pub fn handle_new_tab(self: &Rc<Self>) {
        if self.terminal_dock.has_focus() {
            return;
        }
        let panel = self.commander.borrow().state().active_panel;
        let (update, request) = {
            let mut commander = self.commander.borrow_mut();
            let update = commander.open_tab(panel);
            let request = crate::application::refresh_request(
                &commander,
                panel,
                rust_i18n::t!("status.loading_directory").into_owned(),
            );
            (update, request)
        };
        self.apply_update(update);
        self.navigation_controller().start_directory_load(request);
    }

    pub fn handle_close_tab(self: &Rc<Self>) {
        if self.terminal_dock.has_focus() {
            return;
        }
        let panel = self.commander.borrow().state().active_panel;
        let update = {
            let mut commander = self.commander.borrow_mut();
            commander.close_tab(panel)
        };
        self.apply_update(update);
    }

    pub fn handle_next_tab(self: &Rc<Self>) {
        if self.terminal_dock.has_focus() {
            return;
        }
        let panel = self.commander.borrow().state().active_panel;
        let (update, needs_load) = {
            let mut commander = self.commander.borrow_mut();
            let update = commander.next_tab(panel);
            let needs_load = commander.state().panel(panel).entries.is_empty();
            (update, needs_load)
        };
        self.apply_update(update);
        if needs_load {
            let request = {
                let commander = self.commander.borrow();
                crate::application::refresh_request(
                    &commander,
                    panel,
                    rust_i18n::t!("status.loading_directory").into_owned(),
                )
            };
            self.navigation_controller().start_directory_load(request);
        }
    }

    pub fn handle_prev_tab(self: &Rc<Self>) {
        if self.terminal_dock.has_focus() {
            return;
        }
        let panel = self.commander.borrow().state().active_panel;
        let (update, needs_load) = {
            let mut commander = self.commander.borrow_mut();
            let update = commander.prev_tab(panel);
            let needs_load = commander.state().panel(panel).entries.is_empty();
            (update, needs_load)
        };
        self.apply_update(update);
        if needs_load {
            let request = {
                let commander = self.commander.borrow();
                crate::application::refresh_request(
                    &commander,
                    panel,
                    rust_i18n::t!("status.loading_directory").into_owned(),
                )
            };
            self.navigation_controller().start_directory_load(request);
        }
    }

    pub fn handle_mark_all(self: &Rc<Self>) {
        if self.terminal_dock.has_focus() {
            return;
        }
        let panel = self.commander.borrow().state().active_panel;
        {
            let mut commander = self.commander.borrow_mut();
            let _ = commander.mark_all(panel);
        }
        self.apply_panel_selection(panel);
    }

    pub fn handle_open_active(self: &Rc<Self>) {
        if self.terminal_dock.has_focus() {
            return;
        }
        let active_panel = self.commander.borrow().state().active_panel;
        self.navigation_controller()
            .start_selected_navigation(active_panel);
    }

    pub fn handle_copy(self: &Rc<Self>) {
        self.operations_controller()
            .handle_operation(FileOperationKind::Copy);
    }

    pub fn handle_move(self: &Rc<Self>) {
        self.operations_controller()
            .handle_operation(FileOperationKind::Move);
    }

    pub fn handle_delete(self: &Rc<Self>) {
        self.operations_controller()
            .handle_operation(FileOperationKind::Delete);
    }

    pub fn handle_delete_permanent(self: &Rc<Self>) {
        self.operations_controller().handle_delete_permanent();
    }

    pub fn handle_quit(self: &Rc<Self>) {
        // Defer close: destroying from inside a Capture key handler can abort on Windows.
        let window = self.window.clone();
        glib::idle_add_local_once(move || {
            window.close();
        });
    }

    pub fn handle_go_location_root(self: &Rc<Self>) {
        if self.terminal_dock.has_focus() {
            return;
        }

        let panel = self.commander.borrow().state().active_panel;
        let (request, already_at_root_update) = {
            let mut commander = self.commander.borrow_mut();
            let root = commander.state().panel(panel).location.root();
            if commander.state().panel(panel).location.history_key() == root.history_key() {
                let update = if commander.state().panel(panel).entries.is_empty() {
                    commander.set_status(
                        t!("status.opened_location_root", path = root.display_label()).into_owned(),
                    )
                } else {
                    let mut update = commander.select_single(panel, 0);
                    commander.set_status(
                        t!("status.opened_location_root", path = root.display_label()).into_owned(),
                    );
                    update.status = true;
                    update
                };
                (None, Some(update))
            } else {
                (
                    Some(location_root_navigation_request(&commander, panel)),
                    None,
                )
            }
        };

        if let Some(update) = already_at_root_update {
            self.apply_update(update);
            self.focus_active_panel();
            return;
        }

        if let Some(request) = request {
            self.navigation_controller().start_directory_load(request);
        }
    }

    pub fn handle_favorites(self: &Rc<Self>) {
        if self.terminal_dock.has_focus() {
            return;
        }

        let favorites = self.config_store.snapshot().favorites;
        let active_panel = self.commander.borrow().state().active_panel;
        let current_directory = self.commander.borrow().panel_directory(active_panel);
        let this = Rc::clone(self);
        dialogs::show_favorites(&self.window, favorites, current_directory, move |action| {
            match action {
                dialogs::FavoritesAction::Go(path) => {
                    let request = filesystem_navigation_request(active_panel, path);
                    this.navigation_controller().start_directory_load(request);
                }
                dialogs::FavoritesAction::Update(favorites) => {
                    if let Err(error) = this.config_store.update(|config| {
                        config.favorites = favorites;
                    }) {
                        dialogs::show_error(
                            &this.window,
                            &t!("error.could_not_save_settings"),
                            &error.to_string(),
                        );
                    }
                }
            }
        });
    }

    pub fn handle_find_files(self: &Rc<Self>) {
        if self.terminal_dock.has_focus() {
            return;
        }

        let active_panel = self.commander.borrow().state().active_panel;
        let initial_root = self
            .commander
            .borrow()
            .state()
            .panel(active_panel)
            .location
            .filesystem_path()
            .map(|path| path.to_path_buf());
        let show_hidden_files = self.app_config_cache.borrow().panels.show_hidden_files;
        let this = Rc::clone(self);
        dialogs::show_find_files(
            &self.window,
            initial_root,
            show_hidden_files,
            move |action| match action {
                dialogs::SearchDialogAction::GoTo {
                    directory,
                    focus_name,
                } => {
                    let request =
                        dialogs::search_go_to_request(active_panel, directory, focus_name);
                    this.navigation_controller().start_directory_load(request);
                }
            },
        );
    }
}
