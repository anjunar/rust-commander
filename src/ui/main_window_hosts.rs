use crate::application::{ActivePanel, ViewUpdate};

pub trait ViewHost {
    fn apply_update(&self, update: ViewUpdate);
    fn show_error(&self, title: &str, detail: &str);
    fn set_status(&self, status: String);
    /// Update cursor highlight and marks without rebuilding the list or scrolling.
    fn apply_panel_selection(&self, panel: ActivePanel);
}

pub trait NavigationHost: ViewHost {
    fn set_navigation_busy(&self, panel: ActivePanel, busy: bool, message: &str);
    fn focus_active_panel(&self);
    fn apply_panel_root(&self, panel: ActivePanel);
    fn notify_initial_panel_loaded(&self, panel: ActivePanel);
    fn update_panel_row_size(&self, panel: ActivePanel, index: usize, size_label: &str);
}

pub trait OperationsHost: ViewHost {}

pub trait TerminalHost: ViewHost {
    fn focus_active_panel(&self);
}
