use gtk::prelude::*;

use crate::{
    application::{ActivePanel, AppState},
    ui::file_panel_view::FilePanelView,
};

pub struct CommanderView {
    pub root: gtk::Box,
    pub left: FilePanelView,
    pub right: FilePanelView,
}

impl CommanderView {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        root.set_hexpand(true);
        root.set_vexpand(true);
        root.set_homogeneous(true);
        root.add_css_class("commander-view");

        let left = FilePanelView::new(ActivePanel::Left);
        let right = FilePanelView::new(ActivePanel::Right);

        root.append(&left.root);
        root.append(&right.root);

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
        panel_view.set_path(&panel_state.path);
        panel_view.set_entries(
            &panel_state.path,
            &panel_state.entries,
            panel_state.selection_indices(),
        );
    }

    pub fn apply_roots(&self, state: &AppState) {
        self.left
            .set_roots(&state.roots, state.selected_root_index(ActivePanel::Left));
        self.right
            .set_roots(&state.roots, state.selected_root_index(ActivePanel::Right));
    }

    pub fn apply_active_panel(&self, active_panel: ActivePanel) {
        self.left.set_active(active_panel == ActivePanel::Left);
        self.right.set_active(active_panel == ActivePanel::Right);
        self.panel(active_panel).grab_focus();
    }
}

impl Default for CommanderView {
    fn default() -> Self {
        Self::new()
    }
}
