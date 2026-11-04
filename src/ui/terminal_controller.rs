use std::{
    path::{Path, PathBuf},
    rc::Rc,
};

use gtk::prelude::*;

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
use gtk::{gdk, gio, glib, pango};
#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
use rust_i18n::t;
#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
use vte4::prelude::*;

use crate::ui::terminal_state::TerminalState;

#[derive(Clone, Copy, Debug)]
pub enum TerminalCommand {
    ToggleVisibility,
    Show,
    Hide,
    Focus,
    RestartInPanelDir,
    Clear,
}

#[derive(Clone, Debug)]
pub enum TerminalAction {
    None,
    FocusPanels,
    ShowError(String),
}

pub struct TerminalController {
    state: Rc<TerminalState>,
    backend: TerminalBackend,
}

impl TerminalController {
    pub fn new(initial_dir: PathBuf) -> Self {
        let state = Rc::new(TerminalState::new(initial_dir));
        let backend = TerminalBackend::new(Rc::clone(&state));
        Self { state, backend }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.backend.widget()
    }

    pub fn state(&self) -> Rc<TerminalState> {
        Rc::clone(&self.state)
    }

    pub fn is_supported(&self) -> bool {
        self.backend.is_supported()
    }

    pub fn has_focus(&self) -> bool {
        self.backend.widget().has_focus()
    }

    pub fn set_panel_dir(&self, path: &Path) {
        self.state.last_panel_dir.set(path.to_path_buf());
    }

    pub fn run(&self, command: TerminalCommand) -> TerminalAction {
        match command {
            TerminalCommand::ToggleVisibility => {
                if self.state.visible.get() {
                    self.run(TerminalCommand::Hide)
                } else {
                    self.run(TerminalCommand::Show)
                }
            }
            TerminalCommand::Show => {
                if !self.state.has_spawned.get() {
                    let panel_dir = self.state.last_panel_dir.get();
                    if let Err(error) = self.backend.spawn(&panel_dir) {
                        return TerminalAction::ShowError(error);
                    }
                }
                self.state.visible.set(true);
                TerminalAction::None
            }
            TerminalCommand::Hide => {
                self.state.visible.set(false);
                TerminalAction::None
            }
            TerminalCommand::Focus => {
                let action = self.run(TerminalCommand::Show);
                if matches!(action, TerminalAction::ShowError(_)) {
                    return action;
                }
                self.backend.widget().grab_focus();
                TerminalAction::None
            }
            TerminalCommand::RestartInPanelDir => {
                let panel_dir = self.state.last_panel_dir.get();
                if let Err(error) = self.backend.spawn(&panel_dir) {
                    return TerminalAction::ShowError(error);
                }
                self.state.visible.set(true);
                self.backend.widget().grab_focus();
                TerminalAction::None
            }
            TerminalCommand::Clear => {
                self.backend.clear();
                TerminalAction::None
            }
        }
    }
}

enum TerminalBackend {
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    Vte(LinuxTerminalBackend),
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    Unsupported(UnsupportedTerminalBackend),
}

impl TerminalBackend {
    fn new(state: Rc<TerminalState>) -> Self {
        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        {
            Self::Vte(LinuxTerminalBackend::new(state))
        }

        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            Self::Unsupported(UnsupportedTerminalBackend::new(state))
        }
    }

    fn widget(&self) -> gtk::Widget {
        match self {
            #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
            Self::Vte(backend) => backend.widget.clone().upcast(),
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            Self::Unsupported(backend) => backend.widget.clone().upcast(),
        }
    }

    fn is_supported(&self) -> bool {
        match self {
            #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
            Self::Vte(_) => true,
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            Self::Unsupported(_) => false,
        }
    }

    fn spawn(&self, working_dir: &Path) -> Result<(), String> {
        match self {
            #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
            Self::Vte(backend) => backend.spawn(working_dir),
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            Self::Unsupported(backend) => backend.spawn(working_dir),
        }
    }

    fn clear(&self) {
        match self {
            #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
            Self::Vte(backend) => backend.clear(),
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            Self::Unsupported(_) => {}
        }
    }
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
struct LinuxTerminalBackend {
    widget: vte4::Terminal,
    state: Rc<TerminalState>,
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
impl LinuxTerminalBackend {
    fn new(state: Rc<TerminalState>) -> Self {
        let widget = vte4::Terminal::new();
        widget.set_hexpand(true);
        widget.set_vexpand(true);
        widget.set_scrollback_lines(20_000);
        widget.set_scroll_on_output(false);
        widget.set_scroll_on_keystroke(true);
        widget.set_mouse_autohide(true);
        widget.set_cursor_blink_mode(vte4::CursorBlinkMode::System);
        widget.set_cursor_shape(vte4::CursorShape::Block);
        widget.set_clear_background(false);

        let font = pango::FontDescription::from_string("Monospace 12");
        widget.set_font(Some(&font));
        widget.set_bold_is_bright(false);
        widget.set_cell_width_scale(1.0);
        widget.set_cell_height_scale(1.0);

        let foreground = gdk::RGBA::new(0.839, 0.867, 0.894, 1.0);
        let background = gdk::RGBA::new(0.086, 0.102, 0.122, 1.0);
        let cursor = gdk::RGBA::new(0.839, 0.867, 0.894, 1.0);
        let cursor_foreground = gdk::RGBA::new(0.086, 0.102, 0.122, 1.0);
        let palette = [
            gdk::RGBA::new(0.176, 0.196, 0.231, 1.0),
            gdk::RGBA::new(0.749, 0.380, 0.416, 1.0),
            gdk::RGBA::new(0.639, 0.745, 0.549, 1.0),
            gdk::RGBA::new(0.922, 0.796, 0.545, 1.0),
            gdk::RGBA::new(0.506, 0.631, 0.757, 1.0),
            gdk::RGBA::new(0.706, 0.557, 0.678, 1.0),
            gdk::RGBA::new(0.561, 0.737, 0.733, 1.0),
            gdk::RGBA::new(0.839, 0.867, 0.894, 1.0),
            gdk::RGBA::new(0.302, 0.337, 0.396, 1.0),
            gdk::RGBA::new(0.843, 0.529, 0.561, 1.0),
            gdk::RGBA::new(0.722, 0.808, 0.631, 1.0),
            gdk::RGBA::new(0.945, 0.835, 0.620, 1.0),
            gdk::RGBA::new(0.592, 0.714, 0.835, 1.0),
            gdk::RGBA::new(0.769, 0.647, 0.745, 1.0),
            gdk::RGBA::new(0.635, 0.792, 0.788, 1.0),
            gdk::RGBA::new(0.925, 0.937, 0.953, 1.0),
        ];
        let palette_refs: Vec<&gdk::RGBA> = palette.iter().collect();
        widget.set_colors(Some(&foreground), Some(&background), &palette_refs);
        widget.set_color_bold(Some(&foreground));
        widget.set_color_cursor(Some(&cursor));
        widget.set_color_cursor_foreground(Some(&cursor_foreground));

        let state_for_notify = Rc::clone(&state);
        widget.connect_current_directory_uri_notify(move |terminal| {
            #[allow(deprecated)]
            let Some(uri) = terminal.current_directory_uri() else {
                return;
            };
            let file = gio::File::for_uri(uri.as_str());
            let Some(path) = file.path() else {
                return;
            };
            state_for_notify.working_dir.set(path);
        });

        Self { widget, state }
    }

    fn spawn(&self, working_dir: &Path) -> Result<(), String> {
        let Some(working_directory) = working_dir.to_str() else {
            return Err(t!(
                "terminal.unsupported_path_chars",
                path = working_dir.display().to_string()
            )
            .into_owned());
        };
        let working_directory = working_directory.to_string();
        let working_directory_path = PathBuf::from(&working_directory);

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let argv = [shell.as_str()];

        let state = Rc::clone(&self.state);
        self.widget.spawn_async(
            vte4::PtyFlags::DEFAULT,
            Some(&working_directory),
            &argv,
            &[],
            glib::SpawnFlags::DEFAULT,
            || {},
            -1,
            None::<&gio::Cancellable>,
            move |result| {
                if result.is_ok() {
                    state.has_spawned.set(true);
                    state.working_dir.set(working_directory_path.clone());
                    state.last_panel_dir.set(working_directory_path.clone());
                }
            },
        );

        Ok(())
    }

    fn clear(&self) {
        self.widget.reset(false, true);
    }
}

/// The dock content where no embedded terminal backend exists yet.
#[cfg(any(target_os = "windows", target_os = "macos"))]
struct UnsupportedTerminalBackend {
    widget: gtk::Box,
    description: crate::ui::dsl::Property<String>,
    state: Rc<TerminalState>,
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
impl UnsupportedTerminalBackend {
    fn new(state: Rc<TerminalState>) -> Self {
        use crate::ui::dsl::{render, text, tr, Property};

        let description = Property::new(String::new());
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 12);
        render(&widget, || {
            let placeholder = crate::ui::dsl::adopt(&widget);
            placeholder.set_hexpand(true);
            placeholder.set_vexpand(true);
            placeholder.margin(12);
            placeholder.css("terminal-placeholder");
            placeholder.set_focusable(true);

            text(tr!("terminal.windows_placeholder_title"), |title| {
                title.set_wrap(true);
                title.css("terminal-placeholder-title");
            });
            text(&description, |copy| {
                copy.set_wrap(true);
                copy.css("terminal-placeholder-copy");
            });
        });

        Self {
            widget,
            description,
            state,
        }
    }

    fn spawn(&self, working_dir: &Path) -> Result<(), String> {
        self.state.has_spawned.set(true);
        self.state.working_dir.set(working_dir.to_path_buf());
        self.state.last_panel_dir.set(working_dir.to_path_buf());
        self.description.set(
            rust_i18n::t!(
                "terminal.windows_placeholder_body",
                path = working_dir.display().to_string()
            )
            .into_owned(),
        );
        Ok(())
    }
}
