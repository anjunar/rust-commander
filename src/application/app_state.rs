use std::path::PathBuf;

use rust_i18n::t;

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

    pub fn label(self) -> String {
        match self {
            Self::Left => t!("panel.left").into_owned(),
            Self::Right => t!("panel.right").into_owned(),
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
            status: t!("status.ready").into_owned(),
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

    pub fn status_line(&self) -> String {
        let active = self.active_panel();
        let inactive = self.inactive_panel();
        let active_selected = active.selected_count();
        let inactive_selected = inactive.selected_count();

        match (active_selected, inactive_selected) {
            (0, 0) => self.status.clone(),
            (active_count, 0) => t!(
                "status.selected_active",
                status = self.status.as_str(),
                count = active_count
            )
            .into_owned(),
            (0, inactive_count) => t!(
                "status.selected_inactive",
                status = self.status.as_str(),
                count = inactive_count
            )
            .into_owned(),
            (active_count, inactive_count) => t!(
                "status.selected_both",
                status = self.status.as_str(),
                active = active_count,
                inactive = inactive_count
            )
            .into_owned(),
        }
    }
}
