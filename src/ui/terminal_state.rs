use std::{cell::Cell, path::PathBuf};

use crate::ui::dsl::Property;

/// The terminal's observable state. The dock binds to it, so a directory
/// change reported by the shell or a visibility toggle shows up without
/// anyone refreshing the toolbar.
pub struct TerminalState {
    pub visible: Property<bool>,
    pub working_dir: Property<PathBuf>,
    pub last_panel_dir: Property<PathBuf>,
    pub has_spawned: Cell<bool>,
}

impl TerminalState {
    pub fn new(initial_dir: PathBuf) -> Self {
        Self {
            visible: Property::new(false),
            working_dir: Property::new(initial_dir.clone()),
            last_panel_dir: Property::new(initial_dir),
            has_spawned: Cell::new(false),
        }
    }
}
