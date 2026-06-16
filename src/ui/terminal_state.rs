use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct TerminalState {
    pub visible: bool,
    pub working_dir: PathBuf,
    pub last_panel_dir: PathBuf,
    pub has_spawned: bool,
}

impl TerminalState {
    pub fn new(initial_dir: PathBuf) -> Self {
        Self {
            visible: false,
            working_dir: initial_dir.clone(),
            last_panel_dir: initial_dir,
            has_spawned: false,
        }
    }
}
