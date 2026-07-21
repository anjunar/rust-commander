use gtk::glib;

use crate::{
    application::{ActivePanel, ViewUpdate},
    ui::dialogs,
};

use super::{
    hosts::{NavigationHost, OperationsHost, ViewHost},
    MainWindow,
};

impl ViewHost for MainWindow {
    fn apply_update(&self, update: ViewUpdate) {
        MainWindow::apply_update(self, update);
    }

    fn show_error(&self, title: &str, detail: &str) {
        dialogs::show_error(&self.window, title, detail);
    }

    fn set_status(&self, status: String) {
        self.set_status_message(status);
    }

    fn apply_panel_selection(&self, panel: ActivePanel) {
        MainWindow::apply_panel_selection(self, panel);
    }
}

impl NavigationHost for MainWindow {
    fn set_navigation_busy(&self, panel: ActivePanel, busy: bool, message: &str) {
        MainWindow::set_navigation_busy(self, panel, busy, message);
    }

    fn focus_active_panel(&self) {
        MainWindow::focus_active_panel(self);
    }

    fn apply_panel_root(&self, panel: ActivePanel) {
        self.commander_view
            .apply_root(self.commander.borrow().state(), panel);
    }

    fn notify_initial_panel_loaded(&self, panel: ActivePanel) {
        let callback = {
            let mut state = self.startup_load_state.borrow_mut();
            if !state.wait_for_initial_panels {
                return;
            }

            match panel {
                ActivePanel::Left => state.left_done = true,
                ActivePanel::Right => state.right_done = true,
            }

            if !state.is_complete() {
                return;
            }

            state.wait_for_initial_panels = false;
            state.on_ready.take()
        };

        if let Some(callback) = callback {
            glib::idle_add_local_once(move || {
                callback();
            });
        }
    }

    fn update_panel_row_size(&self, panel: ActivePanel, index: usize, size_label: &str) {
        let panel_view = self.commander_view.panel(panel);
        panel_view.update_row_size(index, size_label);
        let commander = self.commander.borrow();
        panel_view.set_footer(&crate::presentation::panel_footer_line(
            commander.state().panel(panel),
        ));
    }
}

impl OperationsHost for MainWindow {}

impl super::hosts::TerminalHost for MainWindow {
    fn focus_active_panel(&self) {
        MainWindow::focus_active_panel(self);
    }
}
