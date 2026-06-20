use std::rc::Rc;

use gtk::prelude::*;

use crate::{application::FileOperationKind, ui::main_window::MainWindow};

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

    pub fn handle_quit(self: &Rc<Self>) {
        self.window.close();
    }
}
