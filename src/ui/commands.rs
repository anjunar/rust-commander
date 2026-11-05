//! Every command of the main window, declared once.
//!
//! Each entry becomes a window action `win.<action>` with its accelerators.
//! Entries with a bar slot also appear on an F-key button of the command bar,
//! which triggers the same action; label, shortcut and handler cannot drift
//! apart.

use std::rc::Rc;

use crate::ui::main_window::MainWindow;

/// Which modifier the command bar currently shows its F-keys for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommandBarMode {
    Normal,
    Alt,
    Ctrl,
    Shift,
    /// Modifier combinations nothing is bound to: the bar shows no labels.
    Hidden,
}

/// The place of a command on the command bar.
#[derive(Clone, Copy)]
pub(crate) struct BarSlot {
    pub mode: CommandBarMode,
    /// The function key, 1 to 10.
    pub key: u8,
    /// Locale key of the button label.
    pub label: &'static str,
}

pub(crate) struct Command {
    pub action: &'static str,
    pub accels: &'static [&'static str],
    pub slot: Option<BarSlot>,
    /// Whether the action carries a boolean state, shown as a menu check mark.
    pub toggle: bool,
    pub run: fn(&Rc<MainWindow>),
}

impl Command {
    const fn bar(
        key: u8,
        action: &'static str,
        accels: &'static [&'static str],
        label: &'static str,
        run: fn(&Rc<MainWindow>),
    ) -> Self {
        Self::bar_in(CommandBarMode::Normal, key, action, accels, label, run)
    }

    const fn bar_in(
        mode: CommandBarMode,
        key: u8,
        action: &'static str,
        accels: &'static [&'static str],
        label: &'static str,
        run: fn(&Rc<MainWindow>),
    ) -> Self {
        Self {
            action,
            accels,
            slot: Some(BarSlot { mode, key, label }),
            toggle: false,
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
            slot: None,
            toggle: false,
            run,
        }
    }

    const fn toggle(
        action: &'static str,
        accels: &'static [&'static str],
        run: fn(&Rc<MainWindow>),
    ) -> Self {
        Self {
            action,
            accels,
            slot: None,
            toggle: true,
            run,
        }
    }

    /// The detailed action name widgets point at.
    pub fn detailed_name(&self) -> String {
        format!("win.{}", self.action)
    }
}

/// The command on function key `key` in `mode`, if there is one.
pub(crate) fn bar_command(mode: CommandBarMode, key: u8) -> Option<&'static Command> {
    COMMANDS.iter().find(|command| {
        command
            .slot
            .is_some_and(|slot| slot.mode == mode && slot.key == key)
    })
}

/// The command behind `action`.
pub(crate) fn command(action: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|command| command.action == action)
}

pub(crate) const COMMANDS: &[Command] = &[
    // The command bar, without modifiers.
    Command::bar(
        1,
        "settings",
        &["F1"],
        "command.settings",
        MainWindow::handle_help,
    ),
    Command::bar(
        2,
        "rename",
        &["F2"],
        "command.rename",
        MainWindow::handle_rename,
    ),
    Command::bar(3, "view", &["F3"], "command.view", MainWindow::handle_view),
    Command::bar(4, "edit", &["F4"], "command.edit", MainWindow::handle_edit),
    Command::bar(5, "copy", &["F5"], "command.copy", MainWindow::handle_copy),
    Command::bar(
        6,
        "move-files",
        &["F6"],
        "command.move",
        MainWindow::handle_move,
    ),
    Command::bar(
        7,
        "make-directory",
        &["F7"],
        "command.mkdir",
        MainWindow::handle_make_directory,
    ),
    Command::bar(
        8,
        "delete",
        &["F8", "Delete"],
        "command.delete",
        MainWindow::handle_delete,
    ),
    Command::bar(
        9,
        "toggle-terminal",
        &["F9"],
        "command.terminal",
        MainWindow::handle_toggle_terminal,
    ),
    Command::bar(
        10,
        "quit",
        &["F10"],
        "command.quit",
        MainWindow::handle_quit,
    ),
    // The command bar while Alt is held.
    Command::bar_in(
        CommandBarMode::Alt,
        1,
        "select-root-left",
        &["<Alt>F1"],
        "command.drive_left",
        MainWindow::handle_select_root_left,
    ),
    Command::bar_in(
        CommandBarMode::Alt,
        2,
        "select-root-right",
        &["<Alt>F2"],
        "command.drive_right",
        MainWindow::handle_select_root_right,
    ),
    // The command bar while Ctrl is held.
    Command::bar_in(
        CommandBarMode::Ctrl,
        3,
        "sort-name",
        &["<Primary>F3"],
        "command.sort_name",
        MainWindow::handle_sort_by_name,
    ),
    Command::bar_in(
        CommandBarMode::Ctrl,
        4,
        "sort-extension",
        &["<Primary>F4"],
        "command.sort_extension",
        MainWindow::handle_sort_by_extension,
    ),
    Command::bar_in(
        CommandBarMode::Ctrl,
        5,
        "sort-date",
        &["<Primary>F5"],
        "command.sort_date",
        MainWindow::handle_sort_by_date,
    ),
    Command::bar_in(
        CommandBarMode::Ctrl,
        6,
        "sort-size",
        &["<Primary>F6"],
        "command.sort_size",
        MainWindow::handle_sort_by_size,
    ),
    // The command bar while Shift is held.
    Command::bar_in(
        CommandBarMode::Shift,
        4,
        "create-text-file",
        &["<Shift>F4"],
        "command.new_file",
        MainWindow::handle_create_text_file,
    ),
    Command::bar_in(
        CommandBarMode::Shift,
        5,
        "copy-with-rename",
        &["<Shift>F5"],
        "command.copy_as",
        MainWindow::handle_copy_with_rename,
    ),
    Command::bar_in(
        CommandBarMode::Shift,
        8,
        "delete-permanent",
        &["<Shift>F8", "<Shift>Delete"],
        "command.delete_permanent",
        MainWindow::handle_delete_permanent,
    ),
    // Shortcuts and menu items.
    Command::key("about", &[], MainWindow::handle_about),
    Command::key(
        "properties",
        &["<Alt>Return"],
        MainWindow::handle_properties,
    ),
    Command::key(
        "connect-remote",
        &["F11"],
        MainWindow::handle_connect_remote,
    ),
    Command::key(
        "clipboard-copy",
        &["<Primary>c"],
        MainWindow::handle_clipboard_copy,
    ),
    Command::key(
        "clipboard-cut",
        &["<Primary>x"],
        MainWindow::handle_clipboard_cut,
    ),
    Command::key(
        "clipboard-paste",
        &["<Primary>v"],
        MainWindow::handle_clipboard_paste,
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
    Command::key(
        "swap-panels",
        &["<Primary>u"],
        MainWindow::handle_swap_panels,
    ),
    Command::key("mark-all", &["<Primary>a"], MainWindow::handle_mark_all),
    Command::key("new-tab", &["<Primary>t"], MainWindow::handle_new_tab),
    Command::key("close-tab", &["<Primary>w"], MainWindow::handle_close_tab),
    Command::key(
        "next-tab",
        &["<Primary>Tab", "<Primary>Page_Down"],
        MainWindow::handle_next_tab,
    ),
    Command::key(
        "prev-tab",
        &["<Primary><Shift>Tab", "<Primary>Page_Up"],
        MainWindow::handle_prev_tab,
    ),
    Command::key(
        "remember-folder-sort",
        &[],
        MainWindow::handle_remember_folder_sort,
    ),
    Command::key(
        "forget-folder-sort",
        &[],
        MainWindow::handle_forget_folder_sort,
    ),
    Command::key("favorites", &["<Primary>d"], MainWindow::handle_favorites),
    Command::key(
        "find-files",
        &["<Alt>F7", "<Primary><Shift>f"],
        MainWindow::handle_find_files,
    ),
    Command::key("refresh", &["<Primary>r"], MainWindow::handle_refresh),
    Command::key(
        "go-location-root",
        &["<Primary>backslash"],
        MainWindow::handle_go_location_root,
    ),
    Command::toggle(
        "toggle-hidden-files",
        &["<Primary>period"],
        MainWindow::handle_toggle_hidden_files,
    ),
    Command::key(
        "focus-active-panel",
        &[],
        MainWindow::handle_focus_active_panel,
    ),
    Command::key("console", &[], MainWindow::handle_open_console),
    Command::key("open", &[], MainWindow::handle_open_active),
];

#[cfg(test)]
#[path = "../../tests/unit/ui_commands_tests.rs"]
mod tests;
