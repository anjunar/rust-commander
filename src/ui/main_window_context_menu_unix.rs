//! The in-app context menu of Linux and macOS, as one menu block.
//!
//! It compiles on every platform, so a Windows build still checks it; only
//! Linux and macOS open it.

#![cfg_attr(target_os = "windows", allow(dead_code))]

use std::{path::PathBuf, rc::Rc};

use rust_i18n::t;

use crate::ui::dsl::Menu;

use super::ContextMenuActions;

/// Entries for the selection: opening and renaming need exactly one entry,
/// file operations at least one, creating a directory none.
pub(super) fn compose(menu: &Menu, actions: &ContextMenuActions, selected: &[PathBuf]) {
    let run = |action: &Rc<dyn Fn()>| {
        let action = Rc::clone(action);
        move || action()
    };
    let run_on_selection = |action: &Rc<dyn Fn(Vec<PathBuf>)>| {
        let action = Rc::clone(action);
        let paths = selected.to_vec();
        move || action(paths.clone())
    };

    if selected.len() == 1 {
        menu.item(t!("common.open"), run(&actions.open));
        menu.item(t!("common.rename"), run(&actions.rename));
    }

    if !selected.is_empty() {
        menu.item(t!("operation.copy"), run(&actions.copy));
        menu.item(t!("operation.move"), run(&actions.move_entry));
        menu.destructive(t!("operation.delete"), run(&actions.delete));
    }

    menu.item(t!("command.mkdir"), run(&actions.mkdir));

    if !selected.is_empty() {
        menu.separator();
        menu.item(t!("dialog.chmod_title"), run_on_selection(&actions.chmod));
        menu.item(t!("dialog.chown_title"), run_on_selection(&actions.chown));
    }
}
