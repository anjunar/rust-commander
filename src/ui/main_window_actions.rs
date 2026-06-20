use std::rc::Rc;

#[cfg(not(target_os = "windows"))]
use std::path::PathBuf;

use gtk::prelude::*;
use rust_i18n::t;

use crate::{
    archive::ArchiveService,
    config,
    domain::operation::FileOperationKind,
    remote::RemoteProfile,
    ui::{dialogs, editor_dialog, file_viewer_dialog, main_window::MainWindow},
};

impl MainWindow {
    pub fn handle_switch_panel(self: &Rc<Self>) {
        let update = {
            let mut commander = self.commander.borrow_mut();
            commander.switch_panel()
        };
        self.apply_update(update);
    }

    pub fn handle_open_active(self: &Rc<Self>) {
        if self.terminal_dock.has_focus() {
            return;
        }
        let active_panel = self.commander.borrow().state().active_panel;
        self.navigation_controller()
            .start_selected_navigation(active_panel);
    }

    pub fn handle_view(self: &Rc<Self>) {
        let selected = self
            .commander
            .borrow()
            .state()
            .active_panel()
            .selected_item();

        let Some(selected) = selected else {
            dialogs::show_error(
                &self.window,
                &t!("error.view_unavailable"),
                &t!("error.no_entry_selected"),
            );
            return;
        };

        if selected.is_parent_link {
            dialogs::show_error(
                &self.window,
                &t!("error.view_unavailable"),
                &t!("error.parent_cannot_be_viewed"),
            );
            return;
        }

        if selected.is_dir {
            dialogs::show_error(
                &self.window,
                &t!("error.view_unavailable"),
                &t!("error.directory_cannot_be_viewed"),
            );
            return;
        }

        if selected.archive_path.is_some() {
            dialogs::show_error(
                &self.window,
                &t!("error.view_unavailable"),
                &t!("error.archive_view_not_wired"),
            );
            return;
        }

        let Some(path) = selected.filesystem_path.clone() else {
            dialogs::show_error(
                &self.window,
                &t!("error.view_unavailable"),
                "Viewing remote files is not available yet.",
            );
            return;
        };

        if let Err(error) = file_viewer_dialog::open(
            &self.window,
            path,
            self.app_config_cache.borrow().viewer.clone(),
        ) {
            dialogs::show_error(
                &self.window,
                &t!("error.could_not_open_viewer"),
                &error.to_string(),
            );
        }
    }

    pub fn handle_rename(self: &Rc<Self>) {
        let selected = self
            .commander
            .borrow()
            .state()
            .active_panel()
            .selected_item();

        let Some(selected) = selected else {
            dialogs::show_error(
                &self.window,
                &t!("error.rename_unavailable"),
                &t!("error.no_entry_selected"),
            );
            return;
        };

        if selected.is_parent_link {
            dialogs::show_error(
                &self.window,
                &t!("error.rename_unavailable"),
                &t!("error.parent_cannot_be_renamed"),
            );
            return;
        }

        let this = Rc::clone(self);
        dialogs::prompt_rename(&self.window, selected.display_name, move |new_name| {
            let renamed_path = selected
                .filesystem_path
                .clone()
                .map(|path| path.with_file_name(new_name.trim()))
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| new_name.trim().to_string());
            let result = {
                let mut commander = this.commander.borrow_mut();
                commander.rename_active(&new_name)
            };
            match result {
                Ok(update) => this.apply_update(update),
                Err(error) => {
                    this.show_command_failed(error);
                    return;
                }
            }
            this.set_status_message(t!("status.renamed", path = renamed_path).into_owned());
        });
    }

    pub fn handle_open_console(self: &Rc<Self>) {
        let path = self
            .commander
            .borrow()
            .state()
            .active_panel()
            .location
            .host_directory();

        let Some(path) = path else {
            dialogs::show_error(
                &self.window,
                &t!("error.could_not_open_console"),
                "A local terminal can only be opened from filesystem or archive views.",
            );
            return;
        };

        if let Err(error) = crate::platform::open_console(&path) {
            dialogs::show_error(
                &self.window,
                &t!("error.could_not_open_console"),
                &error.to_string(),
            );
            return;
        }

        let update = {
            let mut commander = self.commander.borrow_mut();
            commander.set_status(
                t!(
                    "status.console_opened_at",
                    path = path.display().to_string()
                )
                .into_owned(),
            )
        };
        self.apply_update(update);
    }

    pub fn handle_help(self: &Rc<Self>) {
        let current_config = self.app_config_cache.borrow().clone();
        let this = Rc::clone(self);
        dialogs::show_settings(&self.window, current_config, move |next_config| {
            if let Err(error) = config::save(&next_config) {
                dialogs::show_error(
                    &this.window,
                    &t!("error.could_not_save_settings"),
                    &error.to_string(),
                );
                return;
            }

            this.app_config_cache.replace(next_config.clone());
            this.archive_service
                .replace(ArchiveService::with_default_backends());
            let selected_locale = crate::i18n::apply_locale(next_config.locale.language.as_deref());

            let update = {
                let mut commander = this.commander.borrow_mut();
                commander.apply_archive_config(next_config.archive.clone());
                let mut update = match commander.apply_panel_settings(next_config.panels.clone()) {
                    Ok(update) => update,
                    Err(error) => {
                        dialogs::show_error(
                            &this.window,
                            &t!("error.could_not_save_settings"),
                            &error.to_string(),
                        );
                        return;
                    }
                };
                commander.set_status(
                    t!(
                        "status.language_changed",
                        language = crate::i18n::locale_display_name(selected_locale)
                    )
                    .into_owned(),
                );
                update.status = true;
                update
            };
            this.apply_update(update);
            this.window_chrome().apply_theme();
            this.window_chrome()
                .refresh_localized_labels(&this.commander_view, &this.terminal_dock);
        });
    }

    pub fn handle_copy(self: &Rc<Self>) {
        self.operations_controller()
            .handle_operation(FileOperationKind::Copy);
    }

    pub fn handle_connect_remote(self: &Rc<Self>) {
        let active_panel = self.commander.borrow().state().active_panel;
        let remote_config = self.app_config_cache.borrow().remote.clone();
        let this = Rc::clone(self);
        dialogs::prompt_remote_connection(&self.window, remote_config, move |action| {
            match action {
                dialogs::RemoteDialogAction::Connect(result) => {
                    if let Some(profile_name) = result.last_used_profile {
                        if let Err(error) =
                            this.update_remote_config(|remote| remote.last_used_profile = Some(profile_name))
                        {
                            dialogs::show_error(
                                &this.window,
                                &t!("error.could_not_save_settings"),
                                &error.to_string(),
                            );
                            return;
                        }
                    }

                    this.navigation_controller()
                        .start_remote_session(active_panel, result.session);
                }
                dialogs::RemoteDialogAction::SaveProfile {
                    profile,
                    previous_name,
                } => {
                    if let Err(error) = this.update_remote_config(|remote| {
                        if let Some(previous_name) = previous_name.as_ref() {
                            if previous_name != &profile.name {
                                remote.profiles.retain(|item| item.name != *previous_name);
                            }
                        }
                        upsert_remote_profile(&mut remote.profiles, profile.clone());
                        remote.last_used_profile = Some(profile.name.clone());
                    }) {
                        dialogs::show_error(
                            &this.window,
                            &t!("error.could_not_save_settings"),
                            &error.to_string(),
                        );
                    }
                }
                dialogs::RemoteDialogAction::DeleteProfile { name } => {
                    if let Err(error) = this.update_remote_config(|remote| {
                        remote.profiles.retain(|profile| profile.name != name);
                        if remote.last_used_profile.as_deref() == Some(name.as_str()) {
                            remote.last_used_profile = None;
                        }
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

    pub fn handle_toggle_terminal(self: &Rc<Self>) {
        if !self.terminal_dock.is_supported() {
            self.handle_open_console();
            return;
        }
        self.terminal_dock.set_panel_dir(self.active_panel_path());
        self.handle_terminal_action(self.terminal_dock.toggle());
    }

    pub fn handle_focus_terminal(self: &Rc<Self>) {
        if !self.terminal_dock.is_supported() {
            self.handle_open_console();
            return;
        }
        self.terminal_dock.set_panel_dir(self.active_panel_path());
        self.handle_terminal_action(self.terminal_dock.focus_terminal());
    }

    pub fn handle_restart_terminal(self: &Rc<Self>) {
        if !self.terminal_dock.is_supported() {
            self.handle_open_console();
            return;
        }
        self.terminal_dock.set_panel_dir(self.active_panel_path());
        self.handle_terminal_action(self.terminal_dock.restart_in_panel_dir());
    }

    pub fn handle_edit(self: &Rc<Self>) {
        let selected = self
            .commander
            .borrow()
            .state()
            .active_panel()
            .selected_item();

        let Some(selected) = selected else {
            dialogs::show_error(
                &self.window,
                &t!("error.edit_unavailable"),
                &t!("error.no_entry_selected"),
            );
            return;
        };

        if selected.is_parent_link {
            dialogs::show_error(
                &self.window,
                &t!("error.edit_unavailable"),
                &t!("error.parent_cannot_be_edited"),
            );
            return;
        }

        if selected.is_dir {
            dialogs::show_error(
                &self.window,
                &t!("error.edit_unavailable"),
                &t!("error.directory_cannot_be_edited"),
            );
            return;
        }

        if selected.archive_path.is_some() {
            dialogs::show_error(
                &self.window,
                &t!("error.edit_unavailable"),
                &t!("error.archive_edit_not_supported"),
            );
            return;
        }

        let Some(path) = selected.filesystem_path.clone() else {
            dialogs::show_error(
                &self.window,
                &t!("error.edit_unavailable"),
                "Editing remote files is not available yet.",
            );
            return;
        };

        let this = Rc::clone(self);
        if let Err(error) = editor_dialog::edit_file(&self.window, path, move |path| {
            this.set_status_message(
                t!("status.saved", path = path.display().to_string()).into_owned(),
            );
        }) {
            dialogs::show_error(
                &self.window,
                &t!("error.could_not_open_editor"),
                &error.to_string(),
            );
        }
    }

    pub fn handle_move(self: &Rc<Self>) {
        self.operations_controller()
            .handle_operation(FileOperationKind::Move);
    }

    pub fn handle_delete(self: &Rc<Self>) {
        self.operations_controller()
            .handle_operation(FileOperationKind::Delete);
    }

    #[cfg(not(target_os = "windows"))]
    pub fn handle_unix_chmod(self: &Rc<Self>, selected_paths: Vec<PathBuf>) {
        let this = Rc::clone(self);
        dialogs::prompt_unix_chmod(
            &self.window,
            selected_paths.clone(),
            move |mode, recursive| {
                if let Err(error) = crate::platform::chmod_paths(&selected_paths, &mode, recursive)
                {
                    this.show_command_failed(error);
                    return;
                }

                this.set_status_message(
                    t!(
                        "status.permissions_updated",
                        count = selected_paths.len(),
                        mode = mode.trim()
                    )
                    .into_owned(),
                );
            },
        );
    }

    #[cfg(not(target_os = "windows"))]
    pub fn handle_unix_chown(self: &Rc<Self>, selected_paths: Vec<PathBuf>) {
        let this = Rc::clone(self);
        dialogs::prompt_unix_chown(
            &self.window,
            selected_paths.clone(),
            move |owner_spec, recursive| {
                if let Err(error) =
                    crate::platform::chown_paths(&selected_paths, &owner_spec, recursive)
                {
                    this.show_command_failed(error);
                    return;
                }

                this.set_status_message(
                    t!(
                        "status.owner_updated",
                        count = selected_paths.len(),
                        owner = owner_spec.trim()
                    )
                    .into_owned(),
                );
            },
        );
    }

    pub fn handle_make_directory(self: &Rc<Self>) {
        let this = Rc::clone(self);
        dialogs::prompt_new_directory(&self.window, move |name| {
            let result = {
                let mut commander = this.commander.borrow_mut();
                commander.create_directory_in_active(&name)
            };
            match result {
                Ok(update) => this.apply_update(update),
                Err(error) => {
                    this.show_command_failed(error);
                    return;
                }
            }
            let changed_path = this.active_panel_path().join(name.trim());
            this.set_status_message(
                t!(
                    "status.created_directory",
                    path = changed_path.display().to_string()
                )
                .into_owned(),
            );
        });
    }

    pub fn handle_quit(self: &Rc<Self>) {
        self.window.close();
    }
}

fn upsert_remote_profile(
    profiles: &mut Vec<RemoteProfile>,
    profile: RemoteProfile,
) {
    if let Some(existing) = profiles.iter_mut().find(|item| item.name == profile.name) {
        *existing = profile;
    } else {
        profiles.push(profile);
        profiles.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    }
}

impl MainWindow {
    fn update_remote_config(
        &self,
        update_remote: impl FnOnce(&mut crate::remote::RemoteConfig),
    ) -> anyhow::Result<()> {
        let mut next_config = self.app_config_cache.borrow().clone();
        update_remote(&mut next_config.remote);
        config::save(&next_config)?;
        self.app_config_cache.replace(next_config);
        Ok(())
    }
}
