use std::rc::Rc;

use anyhow::Context;
use rust_i18n::t;

use crate::ui::{dialogs, editor_dialog, file_viewer_dialog, main_window::MainWindow};

impl MainWindow {
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
            let rename_result = {
                let commander = this.commander.borrow();
                commander.rename_active_request(&new_name)
            };
            let (source, target) = match rename_result {
                Ok(paths) => paths,
                Err(error) => {
                    this.show_command_failed(error);
                    return;
                }
            };

            if let Err(error) = crate::fs::reader::rename_path(&source, &target) {
                this.show_command_failed(error);
                return;
            }

            let result = {
                let mut commander = this.commander.borrow_mut();
                commander.apply_active_rename(
                    &new_name,
                    t!("status.renamed", path = renamed_path.as_str()).into_owned(),
                )
            };
            match result {
                Ok(update) => this.apply_update(update),
                Err(error) => {
                    this.show_command_failed(error);
                }
            }
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

        if let Err(error) = self.platform_port.open_console(&path) {
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

    pub fn handle_make_directory(self: &Rc<Self>) {
        let this = Rc::clone(self);
        dialogs::prompt_new_directory(&self.window, move |name| {
            let target = {
                let commander = this.commander.borrow();
                commander.create_directory_request(&name)
            };
            let target = match target {
                Ok(target) => target,
                Err(error) => {
                    this.show_command_failed(error);
                    return;
                }
            };

            if let Err(error) = std::fs::create_dir(&target)
                .with_context(|| format!("Could not create directory {}", target.display()))
            {
                this.show_command_failed(error);
                return;
            }

            let update = {
                let mut commander = this.commander.borrow_mut();
                commander.set_status(
                    t!(
                        "status.created_directory",
                        path = target.display().to_string()
                    )
                    .into_owned(),
                )
            };
            this.apply_update(update);
        });
    }
}
