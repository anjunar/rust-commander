use std::path::PathBuf;

use crate::domain::{Panel, PanelTabs, RootLocation, TabId};

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
    pub left: PanelTabs,
    pub right: PanelTabs,
    pub roots: Vec<RootLocation>,
    pub active_panel: ActivePanel,
    pub status: String,
}

impl AppState {
    pub fn new(left: Panel, right: Panel, roots: Vec<RootLocation>, status: String) -> Self {
        Self {
            left: PanelTabs::from_panel(left),
            right: PanelTabs::from_panel(right),
            roots,
            active_panel: ActivePanel::Left,
            status,
        }
    }

    pub fn tabs(&self, panel: ActivePanel) -> &PanelTabs {
        match panel {
            ActivePanel::Left => &self.left,
            ActivePanel::Right => &self.right,
        }
    }

    pub fn tabs_mut(&mut self, panel: ActivePanel) -> &mut PanelTabs {
        match panel {
            ActivePanel::Left => &mut self.left,
            ActivePanel::Right => &mut self.right,
        }
    }

    pub fn panel(&self, panel: ActivePanel) -> &Panel {
        self.tabs(panel).active()
    }

    pub fn panel_mut(&mut self, panel: ActivePanel) -> &mut Panel {
        self.tabs_mut(panel).active_mut()
    }

    pub fn active_tab_id(&self, panel: ActivePanel) -> TabId {
        self.tabs(panel).active_id()
    }

    pub fn active_panel(&self) -> &Panel {
        self.panel(self.active_panel)
    }

    pub fn inactive_panel(&self) -> &Panel {
        self.panel(self.active_panel.other())
    }

    pub fn visible_paths(&self) -> Vec<PathBuf> {
        [self.left.active(), self.right.active()]
            .into_iter()
            .filter_map(|panel| panel.location.filesystem_path())
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
