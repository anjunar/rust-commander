use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
};

use gtk::{gdk, glib, prelude::*};

#[cfg(not(target_os = "windows"))]
use gtk::{gio, pango};

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
    state: Rc<RefCell<TerminalState>>,
    backend: TerminalBackend,
}

impl TerminalController {
    pub fn new(initial_dir: PathBuf) -> Self {
        let state = Rc::new(RefCell::new(TerminalState::new(initial_dir)));
        let backend = TerminalBackend::new(Rc::clone(&state));
        Self { state, backend }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.backend.widget()
    }

    pub fn state(&self) -> Rc<RefCell<TerminalState>> {
        Rc::clone(&self.state)
    }

    pub fn current_working_dir(&self) -> PathBuf {
        self.state.borrow().working_dir.clone()
    }

    pub fn is_supported(&self) -> bool {
        self.backend.is_supported()
    }

    pub fn set_panel_dir(&self, path: &Path) {
        self.state.borrow_mut().last_panel_dir = path.to_path_buf();
    }

    pub fn run(&self, command: TerminalCommand) -> TerminalAction {
        match command {
            TerminalCommand::ToggleVisibility => {
                let visible = self.state.borrow().visible;
                if visible {
                    self.run(TerminalCommand::Hide)
                } else {
                    self.run(TerminalCommand::Show)
                }
            }
            TerminalCommand::Show => {
                if !self.state.borrow().has_spawned {
                    let panel_dir = self.state.borrow().last_panel_dir.clone();
                    if let Err(error) = self.backend.spawn(&panel_dir) {
                        return TerminalAction::ShowError(error);
                    }
                }
                self.state.borrow_mut().visible = true;
                TerminalAction::None
            }
            TerminalCommand::Hide => {
                self.state.borrow_mut().visible = false;
                TerminalAction::None
            }
            TerminalCommand::Focus => {
                let action = self.run(TerminalCommand::Show);
                if matches!(action, TerminalAction::ShowError(_)) {
                    return action;
                }
                self.backend.grab_focus();
                TerminalAction::None
            }
            TerminalCommand::RestartInPanelDir => {
                let panel_dir = self.state.borrow().last_panel_dir.clone();
                if let Err(error) = self.backend.spawn(&panel_dir) {
                    return TerminalAction::ShowError(error);
                }
                self.state.borrow_mut().visible = true;
                self.backend.grab_focus();
                TerminalAction::None
            }
            TerminalCommand::Clear => {
                self.backend.clear();
                TerminalAction::None
            }
        }
    }

    pub fn connect_escape_to_focus_panels<F>(&self, f: F)
    where
        F: Fn() + 'static,
    {
        self.backend.connect_escape(f);
    }
}

enum TerminalBackend {
    #[cfg(not(target_os = "windows"))]
    Vte(LinuxTerminalBackend),
    Unsupported(UnsupportedTerminalBackend),
}

impl TerminalBackend {
    fn new(state: Rc<RefCell<TerminalState>>) -> Self {
        #[cfg(not(target_os = "windows"))]
        {
            Self::Vte(LinuxTerminalBackend::new(state))
        }

        #[cfg(target_os = "windows")]
        {
            Self::Unsupported(UnsupportedTerminalBackend::new(state))
        }
    }

    fn widget(&self) -> gtk::Widget {
        match self {
            #[cfg(not(target_os = "windows"))]
            Self::Vte(backend) => backend.widget.clone().upcast(),
            Self::Unsupported(backend) => backend.widget.clone().upcast(),
        }
    }

    fn is_supported(&self) -> bool {
        match self {
            #[cfg(not(target_os = "windows"))]
            Self::Vte(_) => true,
            Self::Unsupported(_) => false,
        }
    }

    fn spawn(&self, working_dir: &Path) -> Result<(), String> {
        match self {
            #[cfg(not(target_os = "windows"))]
            Self::Vte(backend) => backend.spawn(working_dir),
            Self::Unsupported(backend) => backend.spawn(working_dir),
        }
    }

    fn clear(&self) {
        match self {
            #[cfg(not(target_os = "windows"))]
            Self::Vte(backend) => backend.clear(),
            Self::Unsupported(backend) => backend.clear(),
        }
    }

    fn grab_focus(&self) {
        match self {
            #[cfg(not(target_os = "windows"))]
            Self::Vte(backend) => backend.grab_focus(),
            Self::Unsupported(backend) => backend.grab_focus(),
        }
    }

    fn connect_escape<F>(&self, f: F)
    where
        F: Fn() + 'static,
    {
        match self {
            #[cfg(not(target_os = "windows"))]
            Self::Vte(backend) => backend.connect_escape(f),
            Self::Unsupported(backend) => backend.connect_escape(f),
        }
    }
}

#[cfg(not(target_os = "windows"))]
struct LinuxTerminalBackend {
    widget: vte4::Terminal,
    state: Rc<RefCell<TerminalState>>,
}

#[cfg(not(target_os = "windows"))]
impl LinuxTerminalBackend {
    fn new(state: Rc<RefCell<TerminalState>>) -> Self {
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

        let font = pango::FontDescription::from_string("Cascadia Code 11");
        widget.set_font(Some(&font));

        let foreground = gdk::RGBA::new(0.902, 0.929, 0.953, 1.0);
        let background = gdk::RGBA::new(0.063, 0.094, 0.125, 1.0);
        widget.set_colors(Some(&foreground), Some(&background), &[]);

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
            state_for_notify.borrow_mut().working_dir = path;
        });

        Self { widget, state }
    }

    fn spawn(&self, working_dir: &Path) -> Result<(), String> {
        let Some(working_directory) = working_dir.to_str() else {
            return Err(format!(
                "The terminal directory contains unsupported characters: {}",
                working_dir.display()
            ));
        };

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let argv = [shell.as_str()];

        let state = Rc::clone(&self.state);
        self.widget.spawn_async(
            vte4::PtyFlags::DEFAULT,
            Some(working_directory),
            &argv,
            &[],
            glib::SpawnFlags::DEFAULT,
            || {},
            -1,
            None::<&gio::Cancellable>,
            move |result| {
                if let Ok(pid) = result {
                    state.borrow_mut().has_spawned = true;
                    state.borrow_mut().working_dir = PathBuf::from(working_directory);
                    state.borrow_mut().last_panel_dir = PathBuf::from(working_directory);
                    #[allow(deprecated)]
                    {
                        let _ = pid;
                    }
                }
            },
        );

        Ok(())
    }

    fn clear(&self) {
        self.widget.reset(false, true);
    }

    fn grab_focus(&self) {
        self.widget.grab_focus();
    }

    fn connect_escape<F>(&self, f: F)
    where
        F: Fn() + 'static,
    {
        let controller = gtk::EventControllerKey::new();
        controller.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape {
                f();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        self.widget.add_controller(controller);
    }
}

struct UnsupportedTerminalBackend {
    widget: gtk::Box,
    description: gtk::Label,
    state: Rc<RefCell<TerminalState>>,
}

impl UnsupportedTerminalBackend {
    fn new(state: Rc<RefCell<TerminalState>>) -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 12);
        widget.set_hexpand(true);
        widget.set_vexpand(true);
        widget.set_margin_top(12);
        widget.set_margin_bottom(12);
        widget.set_margin_start(12);
        widget.set_margin_end(12);
        widget.add_css_class("terminal-placeholder");
        widget.set_focusable(true);

        let title = gtk::Label::new(Some(
            "Embedded terminal is currently available on Linux builds with VTE.",
        ));
        title.set_wrap(true);
        title.set_xalign(0.0);
        title.add_css_class("terminal-placeholder-title");
        widget.append(&title);

        let description = gtk::Label::new(Some(""));
        description.set_wrap(true);
        description.set_xalign(0.0);
        description.add_css_class("terminal-placeholder-copy");
        widget.append(&description);

        Self {
            widget,
            description,
            state,
        }
    }

    fn spawn(&self, working_dir: &Path) -> Result<(), String> {
        {
            let mut state = self.state.borrow_mut();
            state.has_spawned = true;
            state.working_dir = working_dir.to_path_buf();
            state.last_panel_dir = working_dir.to_path_buf();
        }

        self.description.set_label(&format!(
            "Requested working directory: {}\n\nWhy this is still a placeholder on Windows:\nConPTY supplies a pseudoconsole stream, but the host application must render terminal output and collect terminal input itself. GTK4 does not provide a native Windows terminal widget comparable to VTE, and this project intentionally avoids building its own emulator.\n\nThe dock architecture is already separated so a future Windows-native terminal control can be added without changing the surrounding commander UI.",
            working_dir.display()
        ));
        Ok(())
    }

    fn clear(&self) {}

    fn grab_focus(&self) {
        self.widget.grab_focus();
    }

    fn connect_escape<F>(&self, f: F)
    where
        F: Fn() + 'static,
    {
        let controller = gtk::EventControllerKey::new();
        controller.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape {
                f();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        self.widget.add_controller(controller);
    }
}
