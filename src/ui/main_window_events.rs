//! What the shell reports, routed to the window's controllers.

use std::rc::{Rc, Weak};

use gtk::glib;
use rust_i18n::t;

use crate::{
    application::ActivePanel,
    domain::sorting::{SortColumn, SortDirection},
    presentation,
    ui::{
        commands::Command, file_panel_view::PanelKeyNavigation, shell::ShellEvents,
        terminal_controller::TerminalAction,
    },
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
            // Focus also moves while the state is being applied; that must
            // not ask for the commander mutably.
            if window.commander.borrow().state().active_panel == panel {
                return;
            }
            let update = window.commander.borrow_mut().set_active_panel(panel);
            window.apply_update(update);
        });
    }

    fn cursor_changed(&self, panel: ActivePanel, index: usize) {
        self.with(move |window| {
            let update = window.commander.borrow_mut().set_cursor(panel, index);
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

    fn panel_key(&self, panel: ActivePanel, action: PanelKeyNavigation) {
        match action {
            PanelKeyNavigation::ToggleMark => self.with(|window| {
                let _ = window.commander.borrow_mut().toggle_mark(panel);
                window.apply_panel_selection(panel);
            }),
            PanelKeyNavigation::InvertMarks => self.with(|window| {
                let _ = window.commander.borrow_mut().invert_marks(panel);
                window.apply_panel_selection(panel);
            }),
            PanelKeyNavigation::SpaceAction => {
                self.with(|window| window.navigation_controller().handle_space_action(panel));
            }
            PanelKeyNavigation::ContextMenu => self.later(move |window| {
                let index = window
                    .commander
                    .borrow()
                    .state()
                    .panel(panel)
                    .cursor_index();
                let anchor = window.view.panel(panel).table_widget();
                window
                    .context_menu_controller()
                    .handle_panel_context_menu(panel, index, &anchor, 32.0, 48.0);
            }),
            PanelKeyNavigation::ActivateSelected => self.later(move |window| {
                window
                    .navigation_controller()
                    .start_selected_navigation(panel)
            }),
            PanelKeyNavigation::EnterSelected => self.later(move |window| {
                window
                    .navigation_controller()
                    .start_enter_selected_directory(panel)
            }),
            PanelKeyNavigation::GoParent => self.later(move |window| {
                window
                    .navigation_controller()
                    .start_parent_navigation(panel)
            }),
        }
    }

    fn quick_search(&self, panel: ActivePanel, query: &str) -> bool {
        let Some(window) = ready(&self.window) else {
            return false;
        };
        if query.is_empty() {
            let update = window
                .commander
                .borrow_mut()
                .set_status(t!("status.ready").into_owned());
            window.apply_update(update);
            return false;
        }
        let (update, matched) = window.commander.borrow_mut().quick_search(panel, query);
        if matched {
            window.apply_update(update);
        }
        matched
    }

    // These rebuild the tab strip, which holds the button being clicked.
    fn tab_activated(&self, panel: ActivePanel, index: usize) {
        self.later(move |window| {
            let update = window.commander.borrow_mut().activate_tab(panel, index);
            window.show_active_tab(panel, update);
        });
    }

    fn tab_new(&self, panel: ActivePanel) {
        self.later(move |window| {
            let update = window.commander.borrow_mut().open_tab(panel);
            window.show_active_tab(panel, update);
        });
    }

    fn tab_close(&self, panel: ActivePanel, index: usize) {
        self.later(move |window| {
            let update = window.commander.borrow_mut().close_tab_at(panel, index);
            window.show_active_tab(panel, update);
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
            window.window_state_controller().persist_pane_session();
        });
    }

    fn terminal_action(&self, action: TerminalAction) {
        self.with(|window| window.handle_terminal_action(action));
    }

    fn focus_panels(&self) {
        self.with(|window| window.focus_active_panel());
    }
}
