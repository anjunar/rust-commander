use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

use gtk::{gdk, EventControllerKey};
use gtk::{gio, glib, prelude::*};
use rust_i18n::t;

use crate::{
    application::ActivePanel,
    domain::{sorting::SortColumn, Entry, PanelLocation, RootLocation},
    ui::{
        columns::{append_file_columns, FileColumnBinding},
        file_row_object::FileRowObject,
    },
};

#[path = "file_panel_view_connections.rs"]
mod connections;
#[path = "file_panel_view_helpers.rs"]
mod helpers;
#[path = "file_panel_view_input.rs"]
mod input;

use helpers::{
    install_root_selector_escape, restore_panel_key_input, schedule_column_view_focus,
    start_inline_rename_with_retry,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PanelKeyNavigation {
    ActivateSelected,
    EnterSelected,
    GoParent,
    ToggleMark,
    SpaceAction,
    InvertMarks,
    ContextMenu,
}

/// Choice from the panel root/drive dropdown.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RootSelectorChoice {
    Drive(usize),
    Sftp,
}

const QUICK_SEARCH_TIMEOUT: Duration = Duration::from_millis(1200);

#[derive(Default)]
pub(super) struct QuickSearchBuffer {
    query: String,
    last_input: Option<Instant>,
}

impl QuickSearchBuffer {
    fn expire_if_needed(&mut self) {
        if self
            .last_input
            .is_some_and(|at| at.elapsed() >= QUICK_SEARCH_TIMEOUT)
        {
            self.clear();
        }
    }

    fn clear(&mut self) {
        self.query.clear();
        self.last_input = None;
    }

    fn set_query(&mut self, query: String) {
        self.query = query;
        self.last_input = Some(Instant::now());
    }
}

pub struct FilePanelView {
    pub root: gtk::Box,
    pub tab_bar: gtk::Box,
    pub root_model: gtk::StringList,
    pub root_dropdown: gtk::DropDown,
    pub path_label: gtk::Label,
    pub store: gio::ListStore,
    pub selection: gtk::SingleSelection,
    pub column_view: gtk::ColumnView,
    pub columns: Vec<FileColumnBinding>,
    pub footer_label: gtk::Label,
    tab_activate_handler: Rc<RefCell<Option<Rc<dyn Fn(usize)>>>>,
    tab_new_handler: Rc<RefCell<Option<Rc<dyn Fn()>>>>,
    ignore_selection: Rc<Cell<bool>>,
    ignore_roots: Rc<Cell<bool>>,
    /// Index of the trailing "SFTP…" action item in the root dropdown, if present.
    sftp_item_index: Rc<Cell<Option<u32>>>,
    /// Last selected drive index (never the SFTP action item).
    last_drive_index: Rc<Cell<Option<u32>>>,
    ignore_sort: Rc<Cell<bool>>,
    last_sort: Rc<RefCell<Option<(SortColumn, gtk::SortType)>>>,
    current_entries_path: Rc<RefCell<Option<PathBuf>>>,
    quick_search: Rc<RefCell<QuickSearchBuffer>>,
    /// Debounce Space (IM commit + key-pressed can both fire on Windows).
    last_space_action: Rc<Cell<Option<Instant>>>,
    inline_rename_cancel: Rc<RefCell<Option<Rc<dyn Fn()>>>>,
    inline_rename_active: Rc<Cell<bool>>,
    panel_im_context: Rc<RefCell<Option<gtk::IMMulticontext>>>,
    panel_key_controller: Rc<RefCell<Option<EventControllerKey>>>,
}

impl FilePanelView {
    pub fn new(panel: ActivePanel) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
        root.set_hexpand(true);
        root.set_vexpand(true);
        root.add_css_class("file-panel");
        match panel {
            ActivePanel::Left => root.add_css_class("file-panel-left"),
            ActivePanel::Right => root.add_css_class("file-panel-right"),
        }

        let tab_bar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        tab_bar.add_css_class("panel-tab-bar");
        tab_bar.set_hexpand(true);
        tab_bar.set_vexpand(false);
        tab_bar.set_valign(gtk::Align::Start);
        root.append(&tab_bar);

        let path_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        path_row.add_css_class("path-row");
        path_row.set_vexpand(false);
        path_row.set_valign(gtk::Align::Start);

        let root_model = gtk::StringList::new(&[]);
        let root_dropdown = gtk::DropDown::new(Some(root_model.clone()), gtk::Expression::NONE);
        root_dropdown.set_width_request(110);
        root_dropdown.set_enable_search(false);
        root_dropdown.add_css_class("root-selector");
        path_row.append(&root_dropdown);

        let path_label = gtk::Label::new(None);
        path_label.set_xalign(0.0);
        path_label.set_hexpand(true);
        path_label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        path_label.add_css_class("path-label");
        path_row.append(&path_label);
        root.append(&path_row);

        let store = gio::ListStore::new::<FileRowObject>();
        let selection = gtk::SingleSelection::new(Some(store.clone()));
        selection.set_autoselect(true);
        selection.set_can_unselect(false);
        let column_view = gtk::ColumnView::new(Some(selection.clone()));
        column_view.set_hexpand(true);
        column_view.set_vexpand(true);
        column_view.set_show_row_separators(false);
        column_view.set_show_column_separators(false);
        column_view.set_single_click_activate(false);
        column_view.set_enable_rubberband(false);
        column_view.add_css_class("file-table");

        let columns = append_file_columns(&column_view);

        let scrolled = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .child(&column_view)
            .build();
        scrolled.add_css_class("panel-scroller");
        root.append(&scrolled);

        let footer_label = gtk::Label::new(None);
        footer_label.set_xalign(0.0);
        footer_label.set_hexpand(true);
        footer_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        footer_label.add_css_class("panel-footer");
        root.append(&footer_label);

        install_root_selector_escape(&root_dropdown, &column_view);

        Self {
            root,
            tab_bar,
            root_model,
            root_dropdown,
            path_label,
            store,
            selection,
            column_view,
            columns,
            footer_label,
            tab_activate_handler: Rc::new(RefCell::new(None)),
            tab_new_handler: Rc::new(RefCell::new(None)),
            ignore_selection: Rc::new(Cell::new(false)),
            ignore_roots: Rc::new(Cell::new(false)),
            sftp_item_index: Rc::new(Cell::new(None)),
            last_drive_index: Rc::new(Cell::new(None)),
            ignore_sort: Rc::new(Cell::new(false)),
            last_sort: Rc::new(RefCell::new(None)),
            current_entries_path: Rc::new(RefCell::new(None)),
            quick_search: Rc::new(RefCell::new(QuickSearchBuffer::default())),
            last_space_action: Rc::new(Cell::new(None)),
            inline_rename_cancel: Rc::new(RefCell::new(None)),
            inline_rename_active: Rc::new(Cell::new(false)),
            panel_im_context: Rc::new(RefCell::new(None)),
            panel_key_controller: Rc::new(RefCell::new(None)),
        }
    }

    pub fn cancel_inline_rename(&self) {
        // Tear down Entry before restoring panel IM — reverse order aborts on Windows.
        if let Some(cancel) = self.inline_rename_cancel.borrow_mut().take() {
            cancel();
        }
        self.inline_rename_active.set(false);
        self.restore_panel_key_input();
    }

    fn suspend_panel_key_input(&self) {
        if let Some(controller) = self.panel_key_controller.borrow().as_ref() {
            // Detach IM so Capture no longer filters printable keys away from Entry.
            controller.set_im_context(Option::<&gtk::IMContext>::None);
        }
        if let Some(im) = self.panel_im_context.borrow().as_ref() {
            im.set_client_widget(None::<&gtk::Widget>);
            im.reset();
        }
    }

    fn restore_panel_key_input(&self) {
        restore_panel_key_input(
            &self.column_view,
            &self.panel_im_context,
            &self.panel_key_controller,
        );
    }

    pub fn begin_inline_rename<F>(&self, index: usize, name: &str, is_dir: bool, on_commit: F)
    where
        F: FnOnce(String) + 'static,
    {
        self.cancel_inline_rename();
        self.inline_rename_active.set(true);
        self.suspend_panel_key_input();

        let Some(row) = self.row_at(index) else {
            self.inline_rename_active.set(false);
            self.restore_panel_key_input();
            return;
        };

        let is_marked = row.is_marked();
        let name = name.to_string();
        // Avoid FOCUS — it steals focus back from the rename Entry.
        self.column_view
            .scroll_to(index as u32, None, gtk::ListScrollFlags::SELECT, None);

        let cancel_slot = Rc::clone(&self.inline_rename_cancel);
        let column_view = self.column_view.clone();
        let active = Rc::clone(&self.inline_rename_active);
        let im = Rc::clone(&self.panel_im_context);
        let keys = Rc::clone(&self.panel_key_controller);

        glib::idle_add_local_once(move || {
            start_inline_rename_with_retry(
                row,
                column_view,
                name,
                is_dir,
                is_marked,
                Rc::new(RefCell::new(Some(on_commit))),
                cancel_slot,
                active,
                im,
                keys,
                10,
            );
        });
    }

    pub fn clear_quick_search(&self) {
        self.quick_search.borrow_mut().clear();
    }

    pub fn set_active(&self, active: bool) {
        if active {
            self.root.add_css_class("active-panel");
        } else {
            self.root.remove_css_class("active-panel");
        }
    }

    pub fn refresh_labels(&self) {
        for binding in &self.columns {
            binding
                .column
                .set_title(Some(&crate::ui::columns::column_title(binding.sort_column)));
        }
    }

    pub fn set_path(&self, label: &str) {
        self.path_label.set_label(label);
    }

    pub fn set_footer(&self, label: &str) {
        self.footer_label.set_label(label);
    }

    pub fn set_tabs(&self, titles: &[(usize, String)], active: usize) {
        while let Some(child) = self.tab_bar.first_child() {
            self.tab_bar.remove(&child);
        }
        for (index, title) in titles {
            let button = gtk::Button::with_label(title);
            button.add_css_class("panel-tab");
            button.set_has_frame(false);
            button.set_can_focus(false);
            button.set_hexpand(false);
            button.set_tooltip_text(Some(title));
            if let Some(label) = button.child().and_downcast::<gtk::Label>() {
                label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                label.set_max_width_chars(24);
            }
            if *index == active {
                button.add_css_class("active");
            }
            let handler = Rc::clone(&self.tab_activate_handler);
            let index = *index;
            button.connect_clicked(move |_| {
                if let Some(callback) = handler.borrow().clone() {
                    callback(index);
                }
            });
            self.tab_bar.append(&button);
        }
        let add = gtk::Button::with_label("+");
        add.add_css_class("panel-tab-add");
        add.set_has_frame(false);
        add.set_can_focus(false);
        add.set_tooltip_text(Some("New tab (Ctrl+T)"));
        let handler = Rc::clone(&self.tab_new_handler);
        add.connect_clicked(move |_| {
            if let Some(callback) = handler.borrow().clone() {
                callback();
            }
        });
        self.tab_bar.append(&add);
    }

    pub fn connect_tab_bar<A, N>(&self, on_activate: A, on_new: N)
    where
        A: Fn(usize) + 'static,
        N: Fn() + 'static,
    {
        *self.tab_activate_handler.borrow_mut() = Some(Rc::new(on_activate));
        *self.tab_new_handler.borrow_mut() = Some(Rc::new(on_new));
    }

    pub fn set_entries(
        &self,
        location: &PanelLocation,
        entries: &[Entry],
        cursor: Option<usize>,
        marks: BTreeSet<usize>,
        size_labels: &[String],
    ) {
        self.cancel_inline_rename();

        let location_label = location.display_label();
        if self.current_entries_path.borrow().as_deref()
            != Some(std::path::Path::new(&location_label))
        {
            self.clear_quick_search();
        }

        let was_ignoring_sort = self.ignore_sort.replace(true);
        self.ignore_selection.set(true);
        self.sync_store(location, entries, size_labels);
        self.apply_marks(&marks);

        if let Some(index) = cursor.filter(|index| *index < entries.len()) {
            self.selection.set_selected(index as u32);
        } else {
            self.selection.set_selected(gtk::INVALID_LIST_POSITION);
        }
        self.ignore_selection.set(false);
        self.ignore_sort.set(was_ignoring_sort);

        // FOCUS keeps keyboard navigation on the selected row (not always row 0).
        if let Some(index) = cursor.filter(|index| *index < entries.len()) {
            schedule_column_view_focus(self.column_view.clone(), index as u32);
        }
    }

    /// Update cursor and mark styling without rebuilding entries or scrolling the list away.
    pub fn apply_cursor_and_marks(&self, cursor: Option<usize>, marks: &BTreeSet<usize>) {
        let n = self.store.n_items() as usize;
        for index in 0..n {
            let Some(row) = self.row_at(index) else {
                continue;
            };
            let marked = marks.contains(&index);
            if row.is_marked() == marked {
                continue;
            }
            row.set_marked(marked);
            crate::ui::columns::refresh_row_visuals(&self.column_view, index, &row);
        }
        if let Some(index) = cursor.filter(|index| *index < n) {
            let index_u32 = index as u32;
            if self.selection.selected() != index_u32 {
                self.ignore_selection.set(true);
                self.selection.set_selected(index_u32);
                self.ignore_selection.set(false);
                schedule_column_view_focus(self.column_view.clone(), index_u32);
            }
        }
    }

    pub fn update_row_size(&self, index: usize, size_label: &str) {
        let Some(row) = self.row_at(index) else {
            return;
        };
        if row.size() == size_label {
            return;
        }
        row.set_size_label(size_label);
        crate::ui::columns::refresh_row_visuals(&self.column_view, index, &row);
    }

    fn apply_marks(&self, marks: &BTreeSet<usize>) {
        let n = self.store.n_items() as usize;
        for index in 0..n {
            let Some(row) = self.row_at(index) else {
                continue;
            };
            let marked = marks.contains(&index);
            if row.is_marked() == marked {
                continue;
            }
            row.set_marked(marked);
            // Full list rebuild path: mutate store so factories rebind.
            // Callers must hold ignore_selection and restore the cursor afterwards.
            self.store.remove(index as u32);
            self.store.insert(index as u32, &row);
        }
    }

    fn sync_store(&self, location: &PanelLocation, entries: &[Entry], size_labels: &[String]) {
        let mut current_entries_path = self.current_entries_path.borrow_mut();
        let location_label = location.display_label();
        let path_changed =
            current_entries_path.as_deref() != Some(std::path::Path::new(&location_label));
        // Full rebuild on path change, or same-path when the list is large (avoids O(n²) refresh).
        let prefer_rebuild = path_changed || entries.len() > 500;
        if prefer_rebuild {
            let objects: Vec<FileRowObject> = entries
                .iter()
                .enumerate()
                .map(|(index, entry)| {
                    let size_label = size_labels
                        .get(index)
                        .map(String::as_str)
                        .unwrap_or_else(|| "-");
                    FileRowObject::new_with_size_label(location, entry, size_label)
                })
                .collect();
            let n = self.store.n_items();
            self.store.splice(0, n, &objects);
            *current_entries_path = Some(PathBuf::from(location_label));
            return;
        }

        let mut index = 0usize;

        while index < entries.len() {
            let entry = &entries[index];
            let size_label = size_labels
                .get(index)
                .map(String::as_str)
                .unwrap_or_else(|| "-");
            let Some(row) = self.row_at(index) else {
                self.store.insert(
                    index as u32,
                    &FileRowObject::new_with_size_label(location, entry, size_label),
                );
                index += 1;
                continue;
            };

            if row.matches_entry(location, entry) {
                if row.size() != size_label {
                    row.set_size_label(size_label);
                    self.store.remove(index as u32);
                    self.store.insert(index as u32, &row);
                }
                index += 1;
                continue;
            }

            if let Some(existing_index) = self.find_matching_row(index + 1, location, entry) {
                for _ in index..existing_index {
                    self.store.remove(index as u32);
                }
                continue;
            }

            self.store.remove(index as u32);
            self.store.insert(
                index as u32,
                &FileRowObject::new_with_size_label(location, entry, size_label),
            );
            index += 1;
        }

        while self.store.n_items() > entries.len() as u32 {
            self.store.remove(entries.len() as u32);
        }
    }

    fn row_at(&self, index: usize) -> Option<FileRowObject> {
        self.store
            .item(index as u32)
            .and_then(|object| object.downcast::<FileRowObject>().ok())
    }

    fn find_matching_row(
        &self,
        start_index: usize,
        location: &PanelLocation,
        entry: &Entry,
    ) -> Option<usize> {
        (start_index..self.store.n_items() as usize).find(|index| {
            self.row_at(*index)
                .map(|row| row.matches_entry(location, entry))
                .unwrap_or(false)
        })
    }

    pub fn set_roots(&self, roots: &[RootLocation], selected_index: Option<usize>) {
        self.ignore_roots.set(true);
        let mut labels = roots
            .iter()
            .map(|root| root.label.clone())
            .collect::<Vec<_>>();
        let sftp_label = t!("panel.root_sftp").into_owned();
        labels.push(sftp_label);
        let label_refs = labels.iter().map(String::as_str).collect::<Vec<_>>();
        self.root_model
            .splice(0, self.root_model.n_items(), label_refs.as_slice());

        let sftp_index = (labels.len() - 1) as u32;
        self.sftp_item_index.set(Some(sftp_index));

        let drive_index = selected_index
            .filter(|index| *index < roots.len())
            .map(|index| index as u32)
            .or_else(|| {
                self.last_drive_index
                    .get()
                    .filter(|index| (*index as usize) < roots.len())
            })
            .or_else(|| (!roots.is_empty()).then_some(0));

        if let Some(index) = drive_index {
            self.root_dropdown.set_selected(index);
            self.last_drive_index.set(Some(index));
        }
        self.ignore_roots.set(false);
    }

    pub fn grab_focus(&self) {
        self.column_view.grab_focus();
        let selected = self.selection.selected();
        if selected != gtk::INVALID_LIST_POSITION {
            schedule_column_view_focus(self.column_view.clone(), selected);
        }
    }

    pub fn refresh_list_after_resize(&self) {
        self.column_view.queue_resize();
        self.column_view.queue_draw();
        let selected = self.selection.selected();
        if selected != gtk::INVALID_LIST_POSITION {
            schedule_column_view_focus(self.column_view.clone(), selected);
        }
    }

    /// Re-resolve icons for every row and push them into visible name cells.
    pub fn reload_icons(&self) {
        let n = self.store.n_items() as usize;
        for index in 0..n {
            let Some(row) = self.row_at(index) else {
                continue;
            };
            row.invalidate_icon();
            row.ensure_icon();
            crate::ui::columns::refresh_row_visuals(&self.column_view, index, &row);
        }
    }

    pub fn open_root_selector(&self) {
        self.root_dropdown.grab_focus();
        self.root_dropdown.emit_activate();
    }

    pub fn set_interaction_enabled(&self, enabled: bool) {
        self.root_dropdown.set_sensitive(enabled);
        self.column_view.set_sensitive(enabled);
    }

    pub fn connect_selection_changed<F>(&self, f: F)
    where
        F: Fn(usize) + 'static,
    {
        let ignore_selection = Rc::clone(&self.ignore_selection);
        let selection = self.selection.clone();
        self.selection.connect_selection_changed(move |_, _, _| {
            if ignore_selection.get() {
                return;
            }
            let selected = selection.selected();
            if selected != gtk::INVALID_LIST_POSITION {
                f(selected as usize);
            }
        });
    }

    pub fn connect_focus_enter<F>(&self, f: F)
    where
        F: Fn() + Clone + 'static,
    {
        let root_dropdown_callback = f.clone();
        self.column_view.connect_has_focus_notify(move |view| {
            if view.has_focus() {
                f();
            }
        });
        self.root_dropdown
            .connect_has_focus_notify(move |dropdown| {
                if dropdown.has_focus() {
                    root_dropdown_callback();
                }
            });
    }

    pub fn connect_primary_click<F>(&self, f: F)
    where
        F: Fn() + 'static,
    {
        let gesture = gtk::GestureClick::new();
        gesture.set_button(gdk::BUTTON_PRIMARY);
        gesture.connect_pressed(move |_, _, _, _| {
            f();
        });
        self.column_view.add_controller(gesture);
    }

    pub fn connect_activate<F>(&self, f: F)
    where
        F: Fn(usize) + 'static,
    {
        self.column_view.connect_activate(move |_, position| {
            f(position as usize);
        });
    }
}
