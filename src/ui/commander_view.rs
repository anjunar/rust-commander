use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;

use crate::{
    application::{ActivePanel, AppState},
    ui::file_panel_view::FilePanelView,
};

/// Pull the divider to exact center when the user drags near the midpoint.
const CENTER_SNAP_IN_PX: i32 = 18;
/// Keep the divider stuck until the drag moves farther out (hysteresis).
const CENTER_SNAP_OUT_PX: i32 = 32;

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
        // Allow panels to shrink below content natural width so folder/tab
        // changes never move the divider away from the user-set position.
        root.set_shrink_start_child(true);
        root.set_shrink_end_child(true);
        root.set_wide_handle(false);
        install_center_snap(&root);

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

    pub fn apply_entries(&self, state: &AppState, panel: ActivePanel) {
        let tabs = state.tabs(panel);
        let panel_state = tabs.active();
        let panel_view = self.panel(panel);
        let tab_titles: Vec<(usize, String)> = tabs
            .tabs()
            .iter()
            .enumerate()
            .map(|(index, tab)| (index, tab.panel.location.tab_title()))
            .collect();
        panel_view.set_tabs(&tab_titles, tabs.active_index());
        panel_view.set_path(&panel_state.location.display_label());
        panel_view.set_footer(&crate::presentation::panel_footer_line(panel_state));
        let size_labels = panel_state
            .entries
            .iter()
            .map(|entry| panel_state.size_label_for(entry))
            .collect::<Vec<_>>();
        panel_view.set_entries(
            &panel_state.location,
            &panel_state.entries,
            panel_state.cursor_index(),
            panel_state.selection_indices(),
            &size_labels,
        );
        panel_view.apply_sort(
            panel_state.sort_state.column,
            panel_state.sort_state.direction,
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

    pub fn refresh_lists_after_layout(&self) {
        self.left.refresh_list_after_resize();
        self.right.refresh_list_after_resize();
    }

    pub fn reload_icons(&self) {
        self.left.reload_icons();
        self.right.reload_icons();
    }
}

impl Default for CommanderView {
    fn default() -> Self {
        Self::new()
    }
}

fn install_center_snap(paned: &gtk::Paned) {
    let snapping = Rc::new(Cell::new(false));
    let latched = Rc::new(Cell::new(false));

    paned.connect_notify_local(Some("position"), move |paned, _| {
        if snapping.get() {
            return;
        }

        let width = paned.width();
        if width <= 0 {
            return;
        }

        let center = width / 2;
        let position = paned.position();
        let distance = (position - center).abs();

        if latched.get() {
            if distance > CENTER_SNAP_OUT_PX {
                latched.set(false);
                return;
            }
        } else if distance <= CENTER_SNAP_IN_PX {
            latched.set(true);
        } else {
            return;
        }

        if position == center {
            return;
        }

        snapping.set(true);
        paned.set_position(center);
        snapping.set(false);
    });
}
