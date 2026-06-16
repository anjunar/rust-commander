use crate::application::ActivePanel;

#[derive(Clone, Copy, Debug, Default)]
pub struct ViewUpdate {
    pub left_entries: bool,
    pub right_entries: bool,
    pub roots: bool,
    pub selection: bool,
    pub status: bool,
    pub active_panel: bool,
}

impl ViewUpdate {
    pub fn all() -> Self {
        Self {
            left_entries: true,
            right_entries: true,
            roots: true,
            selection: true,
            status: true,
            active_panel: true,
        }
    }

    pub fn panel_entries(panel: ActivePanel) -> Self {
        match panel {
            ActivePanel::Left => Self {
                left_entries: true,
                selection: true,
                status: true,
                ..Self::default()
            },
            ActivePanel::Right => Self {
                right_entries: true,
                selection: true,
                status: true,
                ..Self::default()
            },
        }
    }

    pub fn both_panels() -> Self {
        Self {
            left_entries: true,
            right_entries: true,
            selection: true,
            status: true,
            ..Self::default()
        }
    }

    pub fn status() -> Self {
        Self {
            status: true,
            ..Self::default()
        }
    }

    pub fn selection(panel: ActivePanel) -> Self {
        let mut update = Self {
            selection: true,
            active_panel: true,
            status: true,
            ..Self::default()
        };
        match panel {
            ActivePanel::Left => update.left_entries = false,
            ActivePanel::Right => update.right_entries = false,
        }
        update
    }
}
