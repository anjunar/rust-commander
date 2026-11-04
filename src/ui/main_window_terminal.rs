use std::rc::Rc;

use gtk::prelude::*;
use rust_i18n::t;

use crate::ui::{terminal_controller::TerminalAction, terminal_dock::TerminalDock};

use super::hosts::TerminalHost;

const MIN_TERMINAL_HEIGHT: i32 = 180;
const MIN_PANELS_HEIGHT: i32 = 220;

/// Turns what a terminal command decided into window behaviour: room for the
/// dock when it opens, focus back to the panels, or an error dialog.
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

    pub fn handle_terminal_action(&self, action: TerminalAction) {
        match action {
            TerminalAction::None => {
                if self.terminal_dock.is_visible() && self.content_paned.position() < 320 {
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
