use std::{path::PathBuf, rc::Rc};

use gtk::{gdk, prelude::*};

use crate::ui::{
    dsl::{button, hbox, localized_with, mount, text, tr, vbox, IntoText, ReadOnly},
    shell::ShellEvents,
    terminal_controller::{TerminalAction, TerminalCommand, TerminalController},
};

/// The terminal dock under the panels: a toolbar and the terminal backend.
///
/// Visibility, title and working directory are bound to the terminal state,
/// so commands only change that state and the dock follows.
#[derive(Clone)]
pub struct TerminalDock {
    controller: Rc<TerminalController>,
}

/// Composes the dock. `events` receives what the toolbar buttons decide.
pub(crate) fn terminal_dock(
    controller: Rc<TerminalController>,
    events: Rc<dyn ShellEvents>,
) -> TerminalDock {
    let state = controller.state();
    let supported = controller.is_supported();

    vbox(0, |dock| {
        dock.css("terminal-dock");
        dock.set_hexpand(true);
        dock.set_vexpand(true);
        dock.bind_visible(&state.visible);

        hbox(8, |toolbar| {
            toolbar.css("terminal-toolbar");
            toolbar.set_margin_top(8);
            toolbar.set_margin_bottom(8);
            toolbar.set_margin_start(10);
            toolbar.set_margin_end(10);

            text(terminal_title(supported), |title| {
                title.css("terminal-title")
            });
            text(working_directory_text(&controller, supported), |cwd| {
                cwd.set_hexpand(true);
                cwd.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
                cwd.css("terminal-cwd");
            });

            toolbar_button(tr!("terminal.restart"), {
                let controller = Rc::clone(&controller);
                let events = Rc::clone(&events);
                move || events.terminal_action(controller.run(TerminalCommand::RestartInPanelDir))
            });
            toolbar_button(tr!("common.clear"), {
                let controller = Rc::clone(&controller);
                move || {
                    controller.run(TerminalCommand::Clear);
                }
            })
            .set_sensitive(supported);
            toolbar_button(tr!("common.close"), {
                let controller = Rc::clone(&controller);
                let events = Rc::clone(&events);
                move || {
                    controller.run(TerminalCommand::Hide);
                    events.terminal_action(TerminalAction::FocusPanels);
                }
            });
        });

        // Escape leaves the terminal and hands the keyboard back to the panels.
        mount(controller.widget(), |terminal| {
            let events = Rc::clone(&events);
            terminal.on_key_pressed(move |key, _| {
                if key == gdk::Key::Escape {
                    events.focus_panels();
                    true
                } else {
                    false
                }
            });
        });
    });

    TerminalDock { controller }
}

fn toolbar_button(
    label: impl IntoText,
    run: impl Fn() + 'static,
) -> crate::ui::dsl::Ctx<gtk::Button> {
    button(label, |button| {
        button.css("terminal-button");
        button.on_click(run);
    })
}

fn terminal_title(supported: bool) -> ReadOnly<String> {
    if supported {
        return tr!("terminal.title");
    }
    #[cfg(target_os = "macos")]
    {
        tr!("terminal.title_unsupported")
    }
    #[cfg(not(target_os = "macos"))]
    {
        tr!("terminal.title_windows_pending")
    }
}

/// The shell's directory where a terminal runs; otherwise the panel directory
/// it would start in.
fn working_directory_text(controller: &TerminalController, supported: bool) -> ReadOnly<String> {
    let state = controller.state();
    if supported {
        state.working_dir.map(|path| path.display().to_string())
    } else {
        localized_with(&state.last_panel_dir, |path| {
            rust_i18n::t!("terminal.panel_path", path = path.display().to_string()).into_owned()
        })
    }
}

impl TerminalDock {
    pub fn is_supported(&self) -> bool {
        self.controller.is_supported()
    }

    pub fn is_visible(&self) -> bool {
        self.controller.state().visible.get()
    }

    pub fn has_focus(&self) -> bool {
        self.controller.has_focus()
    }

    pub fn set_panel_dir(&self, path: PathBuf) {
        self.controller.set_panel_dir(&path);
    }

    pub fn toggle(&self) -> TerminalAction {
        self.controller.run(TerminalCommand::ToggleVisibility)
    }

    pub fn focus_terminal(&self) -> TerminalAction {
        self.controller.run(TerminalCommand::Focus)
    }

    pub fn restart_in_panel_dir(&self) -> TerminalAction {
        self.controller.run(TerminalCommand::RestartInPanelDir)
    }
}
