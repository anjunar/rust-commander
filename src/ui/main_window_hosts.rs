use crate::application::{ActivePanel, ViewUpdate};

pub trait ViewHost {
    fn apply_update(&self, update: ViewUpdate);
    fn show_error(&self, title: &str, detail: &str);
    fn set_status(&self, status: String);
}

pub trait NavigationHost: ViewHost {
    fn set_navigation_busy(&self, busy: bool, message: &str);
    fn focus_active_panel(&self);
    fn apply_panel_root(&self, panel: ActivePanel);
    fn notify_initial_panel_loaded(&self, panel: ActivePanel);
}

pub trait OperationsHost: ViewHost {}

pub trait TerminalHost: ViewHost {
    fn focus_active_panel(&self);
}
