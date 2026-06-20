use std::rc::Rc;

use gtk::prelude::*;
use rust_i18n::t;

use crate::ui::{terminal_controller::TerminalAction, terminal_dock::TerminalDock};

use super::hosts::TerminalHost;

const MIN_TERMINAL_HEIGHT: i32 = 180;
const MIN_PANELS_HEIGHT: i32 = 220;

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
                if self.terminal_dock.state().borrow().visible
                    && self.content_paned.position() < 320
                {
                    let available_height = self.content_paned.height();
                    if available_height > 0 {
                        let max_position = (available_height - MIN_TERMINAL_HEIGHT).max(0);
                        let preferred_position = (available_height * 2) / 3;
                        let position = if max_position <= MIN_PANELS_HEIGHT {
                            max_position
                        } else {
                            preferred_position.clamp(MIN_PANELS_HEIGHT, max_position)
                        };
                        self.content_paned.set_position(position);
                    }
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
