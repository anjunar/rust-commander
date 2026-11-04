//! What the shell reports, routed to the window's controllers.

use std::rc::{Rc, Weak};

use gtk::glib;
use rust_i18n::t;

use crate::{
    application::ActivePanel,
    domain::sorting::{SortColumn, SortDirection},
    presentation,
    ui::{commands::Command, shell::ShellEvents, terminal_controller::TerminalAction},
};

use super::MainWindow;

pub(super) struct WindowEvents {
    window: Weak<MainWindow>,
}

impl WindowEvents {
    pub(super) fn new(window: Weak<MainWindow>) -> Self {
        Self { window }
    }

    /// Runs `handle` with the window once it is fully built. Signals GTK
    /// emits while the shell is still being composed are dropped.
    fn with(&self, handle: impl FnOnce(&Rc<MainWindow>)) {
        if let Some(window) = ready(&self.window) {
            handle(&window);
        }
    }

    /// Like `with`, on the next main-loop turn, outside the emitting signal.
    fn later(&self, handle: impl FnOnce(&Rc<MainWindow>) + 'static) {
        let window = self.window.clone();
        glib::idle_add_local_once(move || {
            if let Some(window) = ready(&window) {
                handle(&window);
            }
        });
    }
}

fn ready(window: &Weak<MainWindow>) -> Option<Rc<MainWindow>> {
    let window = window.upgrade()?;
    let built = window.controllers.borrow().is_some();
    built.then_some(window)
}

impl ShellEvents for WindowEvents {
    fn run_command(&self, command: &'static Command) {
        self.with(|window| (command.run)(window));
    }

    fn panel_focused(&self, panel: ActivePanel) {
        self.with(|window| {
            let update = {
                let mut commander = window.commander.borrow_mut();
                if commander.state().active_panel == panel {
                    return;
                }
                commander.set_active_panel(panel)
            };
            window.apply_update(update);
        });
    }

    fn selection_changed(&self, panel: ActivePanel, indices: Vec<usize>) {
        self.with(move |window| {
            let update = window.commander.borrow_mut().select_indices(panel, indices);
            window.apply_update(update);
        });
    }

    fn row_activated(&self, panel: ActivePanel, index: usize) {
        self.later(move |window| {
            window
                .navigation_controller()
                .select_single_and_start(panel, index)
        });
    }

    fn open_requested(&self, panel: ActivePanel) {
        self.later(move |window| {
            window
                .navigation_controller()
                .start_selected_navigation(panel)
        });
    }

    fn root_selected(&self, panel: ActivePanel, index: usize) {
        self.later(move |window| {
            window
                .navigation_controller()
                .start_root_navigation(panel, index)
        });
    }

    fn connect_remote(&self, panel: ActivePanel) {
        self.later(move |window| window.handle_connect_remote_for_panel(panel));
    }

    fn context_menu(
        &self,
        panel: ActivePanel,
        row: Option<usize>,
        anchor: gtk::Widget,
        x: f64,
        y: f64,
    ) {
        self.later(move |window| {
            window
                .context_menu_controller()
                .handle_panel_context_menu(panel, row, &anchor, x, y)
        });
    }

    fn sort_changed(&self, panel: ActivePanel, column: SortColumn, direction: SortDirection) {
        self.later(move |window| {
            let status = t!(
                "status.sorted_panel",
                panel = presentation::panel_label(panel),
                column = presentation::sort_column_label(column)
            )
            .into_owned();
            let update = window
                .commander
                .borrow_mut()
                .sort_panel(panel, column, direction, status);
            window.apply_update(update);
        });
    }

    fn terminal_action(&self, action: TerminalAction) {
        self.with(|window| window.handle_terminal_action(action));
    }

    fn focus_panels(&self) {
        self.with(|window| window.focus_active_panel());
    }
}
