//! Every command of the main window, declared once.
//!
//! Each entry becomes a window action `win.<action>` with its accelerators.
//! Entries with a label also get a button in the command bar, which triggers
//! the same action; label, shortcut and handler cannot drift apart.

use std::rc::Rc;

use crate::ui::main_window::MainWindow;

pub(crate) struct Command {
    pub action: &'static str,
    pub accels: &'static [&'static str],
    /// Locale key of the command-bar label; `None` for shortcut-only commands.
    pub label: Option<&'static str>,
    pub run: fn(&Rc<MainWindow>),
}

impl Command {
    const fn bar(
        action: &'static str,
        accels: &'static [&'static str],
        label: &'static str,
        run: fn(&Rc<MainWindow>),
    ) -> Self {
        Self {
            action,
            accels,
            label: Some(label),
            run,
        }
    }

    const fn key(
        action: &'static str,
        accels: &'static [&'static str],
        run: fn(&Rc<MainWindow>),
    ) -> Self {
        Self {
            action,
            accels,
            label: None,
            run,
        }
    }

    /// The detailed action name widgets point at.
    pub fn detailed_name(&self) -> String {
        format!("win.{}", self.action)
    }
}

pub(crate) const COMMANDS: &[Command] = &[
    Command::bar(
        "settings",
        &["F1"],
        "command.settings",
        MainWindow::handle_help,
    ),
    Command::bar(
        "rename",
        &["F2"],
        "command.rename",
        MainWindow::handle_rename,
    ),
    Command::bar("view", &["F3"], "command.view", MainWindow::handle_view),
    Command::bar("edit", &["F4"], "command.edit", MainWindow::handle_edit),
    Command::bar("copy", &["F5"], "command.copy", MainWindow::handle_copy),
    Command::bar(
        "move-files",
        &["F6"],
        "command.move",
        MainWindow::handle_move,
    ),
    Command::bar(
        "make-directory",
        &["F7"],
        "command.mkdir",
        MainWindow::handle_make_directory,
    ),
    Command::bar(
        "delete",
        &["F8"],
        "command.delete",
        MainWindow::handle_delete,
    ),
    Command::bar(
        "toggle-terminal",
        &["F9"],
        "command.terminal",
        MainWindow::handle_toggle_terminal,
    ),
    Command::bar("quit", &["F10"], "command.quit", MainWindow::handle_quit),
    Command::key(
        "connect-remote",
        &["F11"],
        MainWindow::handle_connect_remote,
    ),
    Command::key(
        "focus-terminal",
        &["<Primary>grave"],
        MainWindow::handle_focus_terminal,
    ),
    Command::key(
        "restart-terminal",
        &["<Primary><Shift>grave"],
        MainWindow::handle_restart_terminal,
    ),
    Command::key("switch-panel", &["Tab"], MainWindow::handle_switch_panel),
    Command::key("console", &[], MainWindow::handle_open_console),
    Command::key("open", &[], MainWindow::handle_open_active),
];
