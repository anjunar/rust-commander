use std::path::PathBuf;

use crate::domain::{Panel, RootLocation};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivePanel {
    Left,
    Right,
}

impl ActivePanel {
    pub fn other(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AppState {
    pub left: Panel,
    pub right: Panel,
    pub roots: Vec<RootLocation>,
    pub active_panel: ActivePanel,
    pub status: String,
}

impl AppState {
    pub fn new(left: Panel, right: Panel, roots: Vec<RootLocation>, status: String) -> Self {
        Self {
            left,
            right,
            roots,
            active_panel: ActivePanel::Left,
            status,
        }
    }

    pub fn panel(&self, panel: ActivePanel) -> &Panel {
        match panel {
            ActivePanel::Left => &self.left,
            ActivePanel::Right => &self.right,
        }
    }

    pub fn panel_mut(&mut self, panel: ActivePanel) -> &mut Panel {
        match panel {
            ActivePanel::Left => &mut self.left,
            ActivePanel::Right => &mut self.right,
        }
    }

    pub fn active_panel(&self) -> &Panel {
        self.panel(self.active_panel)
    }

    pub fn inactive_panel(&self) -> &Panel {
        self.panel(self.active_panel.other())
    }

    pub fn visible_paths(&self) -> Vec<PathBuf> {
        [
            self.left.location.filesystem_path(),
            self.right.location.filesystem_path(),
        ]
        .into_iter()
        .flatten()
        .map(PathBuf::from)
        .collect()
    }

    pub fn selected_root_index(&self, panel: ActivePanel) -> Option<usize> {
        let path = self.panel(panel).location.filesystem_path()?;
        self.roots
            .iter()
            .position(|root| path.starts_with(&root.path))
    }
}
