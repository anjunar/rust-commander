use gtk::prelude::*;

use crate::{
    application::{ActivePanel, AppState},
    ui::file_panel_view::FilePanelView,
};

pub struct CommanderView {
    pub root: gtk::Paned,
    pub left: FilePanelView,
    pub right: FilePanelView,
}

impl CommanderView {
    pub fn new() -> Self {
        let root = gtk::Paned::new(gtk::Orientation::Horizontal);
        root.set_hexpand(true);
        root.set_vexpand(true);
        root.add_css_class("commander-view");
        root.set_resize_start_child(true);
        root.set_resize_end_child(true);
        root.set_shrink_start_child(false);
        root.set_shrink_end_child(false);
        root.set_wide_handle(true);

        let left = FilePanelView::new(ActivePanel::Left);
        let right = FilePanelView::new(ActivePanel::Right);

        root.set_start_child(Some(&left.root));
        root.set_end_child(Some(&right.root));

        Self { root, left, right }
    }

    pub fn panel(&self, panel: ActivePanel) -> &FilePanelView {
        match panel {
            ActivePanel::Left => &self.left,
            ActivePanel::Right => &self.right,
        }
    }

    pub fn apply_state(&self, state: &AppState) {
        self.apply_roots(state);
        self.apply_entries(state, ActivePanel::Left);
        self.apply_entries(state, ActivePanel::Right);
        self.apply_active_panel(state.active_panel);
    }

    pub fn apply_entries(&self, state: &AppState, panel: ActivePanel) {
        let panel_state = state.panel(panel);
        let panel_view = self.panel(panel);
        panel_view.set_path(&panel_state.location.display_label());
        panel_view.set_entries(
            &panel_state.location,
            &panel_state.entries,
            panel_state.selection_indices(),
        );
    }

    pub fn apply_root(&self, state: &AppState, panel: ActivePanel) {
        self.panel(panel)
            .set_roots(&state.roots, state.selected_root_index(panel));
    }

    pub fn apply_roots(&self, state: &AppState) {
        self.apply_root(state, ActivePanel::Left);
        self.apply_root(state, ActivePanel::Right);
    }

    pub fn apply_active_panel(&self, active_panel: ActivePanel) {
        self.left.set_active(active_panel == ActivePanel::Left);
        self.right.set_active(active_panel == ActivePanel::Right);
        self.panel(active_panel).grab_focus();
    }

    pub fn focus_active_panel(&self, active_panel: ActivePanel) {
        self.panel(active_panel).grab_focus();
    }

    pub fn refresh_labels(&self) {
        self.left.refresh_labels();
        self.right.refresh_labels();
    }

    pub fn set_interaction_enabled(&self, enabled: bool) {
        self.left.set_interaction_enabled(enabled);
        self.right.set_interaction_enabled(enabled);
    }
}

impl Default for CommanderView {
    fn default() -> Self {
        Self::new()
    }
}
