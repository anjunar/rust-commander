use std::rc::Rc;

#[cfg(not(target_os = "windows"))]
use std::path::PathBuf;

#[cfg(not(target_os = "windows"))]
use rust_i18n::t;

use crate::ui::main_window::MainWindow;

#[cfg(not(target_os = "windows"))]
use crate::ui::dialogs;

impl MainWindow {
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

    #[cfg(not(target_os = "windows"))]
    pub fn handle_unix_chmod(self: &Rc<Self>, selected_paths: Vec<PathBuf>) {
        let this = Rc::clone(self);
        dialogs::prompt_unix_chmod(
            &self.window,
            selected_paths.clone(),
            move |mode, recursive| {
                if let Err(error) =
                    this.platform_port
                        .chmod_paths(&selected_paths, &mode, recursive)
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
                    this.platform_port
                        .chown_paths(&selected_paths, &owner_spec, recursive)
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
}
