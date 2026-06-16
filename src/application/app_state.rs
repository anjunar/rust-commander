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

    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
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
    pub fn new(left: Panel, right: Panel, roots: Vec<RootLocation>) -> Self {
        Self {
            left,
            right,
            roots,
            active_panel: ActivePanel::Left,
            status: "Ready. F3 opens a console, F4 edits, Enter opens, Tab switches panels, F5 copies.".into(),
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
        vec![self.left.path.clone(), self.right.path.clone()]
    }

    pub fn selected_root_index(&self, panel: ActivePanel) -> Option<usize> {
        let path = &self.panel(panel).path;
        self.roots
            .iter()
            .position(|root| path.starts_with(&root.path))
    }

    pub fn status_line(&self) -> String {
        let active = self.active_panel();
        let inactive = self.inactive_panel();
        let active_selected = active.selected_count();
        let inactive_selected = inactive.selected_count();

        match (active_selected, inactive_selected) {
            (0, 0) => self.status.clone(),
            (active_count, 0) => {
                format!("{} | {active_count} selected in the active panel", self.status)
            }
            (0, inactive_count) => {
                format!("{} | {inactive_count} selected in the inactive panel", self.status)
            }
            (active_count, inactive_count) => {
                format!(
                    "{} | {active_count} selected active | {inactive_count} selected inactive",
                    self.status
                )
            }
        }
    }
}
