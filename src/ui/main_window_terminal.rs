use std::rc::Rc;

use rust_i18n::t;

use crate::ui::{terminal_controller::TerminalAction, terminal_dock::TerminalDock};

use super::hosts::TerminalHost;

#[derive(Clone)]
pub struct TerminalController {
    host: Rc<dyn TerminalHost>,
    terminal_dock: TerminalDock,
    content_paned: gtk::Paned,
}

impl TerminalController {
    pub fn new(
        host: Rc<dyn TerminalHost>,
        terminal_dock: TerminalDock,
        content_paned: gtk::Paned,
    ) -> Self {
        Self {
            host,
            terminal_dock,
            content_paned,
        }
    }

    pub fn connect_terminal_dock(&self) {
        {
            let host = Rc::clone(&self.host);
            self.terminal_dock.connect_focus_return(move || {
                host.focus_active_panel();
            });
        }

        {
            let controller = self.clone();
            self.terminal_dock.connect_buttons(move |action| {
                controller.handle_terminal_action(action);
            });
        }
    }

    pub fn handle_terminal_action(&self, action: TerminalAction) {
        match action {
            TerminalAction::None => {
                self.terminal_dock.sync_visibility();
                if self.terminal_dock.state().borrow().visible && self.content_paned.position() < 320
                {
                    self.content_paned.set_position(600);
                }
            }
            TerminalAction::FocusPanels => self.host.focus_active_panel(),
            TerminalAction::ShowError(error) => {
                self.host
                    .show_error(&t!("error.could_not_start_terminal"), &error);
            }
        }
    }
}
