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
    domain::{
        sorting::{SortColumn, SortDirection},
        Entry, PanelLocation, RootLocation,
    },
    ui::{
        columns::{append_file_columns, begin_inline_rename, FileColumnBinding, ROW_POSITION_DATA_KEY},
        file_row_object::FileRowObject,
    },
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
struct QuickSearchBuffer {
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

    pub fn connect_open_key<F, Q>(&self, on_nav: F, on_quick_search: Q)
    where
        F: Fn(PanelKeyNavigation) + 'static,
        Q: Fn(&str) -> bool + 'static,
    {
        let quick_search = Rc::clone(&self.quick_search);
        let on_nav = Rc::new(on_nav);
        let on_quick_search = Rc::new(on_quick_search);
        let last_space_action = Rc::clone(&self.last_space_action);
        let inline_rename_active = Rc::clone(&self.inline_rename_active);
        let emit_space = {
            let on_nav = Rc::clone(&on_nav);
            let last_space_action = Rc::clone(&last_space_action);
            move || {
                let now = Instant::now();
                if last_space_action
                    .get()
                    .is_some_and(|at| now.duration_since(at) < Duration::from_millis(80))
                {
                    return;
                }
                last_space_action.set(Some(now));
                on_nav(PanelKeyNavigation::SpaceAction);
            }
        };
        let emit_space = Rc::new(emit_space);
        let controller = EventControllerKey::new();
        controller.set_propagation_phase(gtk::PropagationPhase::Capture);

        let im_context = gtk::IMMulticontext::new();
        im_context.set_client_widget(Some(&self.column_view));
        controller.set_im_context(Some(&im_context));
        *self.panel_im_context.borrow_mut() = Some(im_context.clone());
        *self.panel_key_controller.borrow_mut() = Some(controller.clone());
        {
            // On Windows the IM often consumes Space as commit(" ") before key-pressed
            // sees it — treat whitespace-only commits as SpaceAction, not quick-search.
            let quick_search = Rc::clone(&quick_search);
            let emit_space = Rc::clone(&emit_space);
            let on_nav = Rc::clone(&on_nav);
            let on_quick_search = Rc::clone(&on_quick_search);
            let inline_rename_active = Rc::clone(&inline_rename_active);
            im_context.connect_commit(move |_, text| {
                if inline_rename_active.get() {
                    return;
                }
                if text.chars().all(|ch| ch.is_whitespace()) {
                    quick_search.borrow_mut().clear();
                    emit_space();
                    return;
                }
                if text == "*" {
                    quick_search.borrow_mut().clear();
                    on_nav(PanelKeyNavigation::InvertMarks);
                    return;
                }
                push_quick_search_text(&quick_search, on_quick_search.as_ref(), text);
            });
        }

        {
            let quick_search = Rc::clone(&quick_search);
            let on_nav = Rc::clone(&on_nav);
            let emit_space = Rc::clone(&emit_space);
            let on_quick_search = Rc::clone(&on_quick_search);
            let inline_rename_active = Rc::clone(&inline_rename_active);
            let column_view = self.column_view.clone();
            controller.connect_key_pressed(move |_, key, _, modifiers| {
                // Let the inline-rename Entry own Left/Right/typing; otherwise Capture
                // steals navigation keys and IM/quick-search can crash the UI.
                if inline_rename_active.get() {
                    return glib::Propagation::Proceed;
                }
                let action = match key {
                    gdk::Key::Return | gdk::Key::KP_Enter => {
                        // Alt+Enter → Properties; stop so ColumnView does not open the item.
                        if modifiers.contains(gdk::ModifierType::ALT_MASK)
                            && !modifiers.intersects(
                                gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SUPER_MASK,
                            )
                        {
                            let _ = WidgetExt::activate_action(
                                &column_view,
                                "win.properties",
                                None,
                            );
                            return glib::Propagation::Stop;
                        }
                        Some(PanelKeyNavigation::ActivateSelected)
                    }
                    gdk::Key::Right | gdk::Key::KP_Right | gdk::Key::rightarrow => {
                        Some(PanelKeyNavigation::EnterSelected)
                    }
                    gdk::Key::Left | gdk::Key::KP_Left | gdk::Key::leftarrow => {
                        Some(PanelKeyNavigation::GoParent)
                    }
                    gdk::Key::space | gdk::Key::KP_Space => {
                        quick_search.borrow_mut().clear();
                        emit_space();
                        return glib::Propagation::Stop;
                    }
                    gdk::Key::Insert | gdk::Key::KP_Insert => Some(PanelKeyNavigation::ToggleMark),
                    gdk::Key::asterisk | gdk::Key::KP_Multiply => {
                        Some(PanelKeyNavigation::InvertMarks)
                    }
                    gdk::Key::Menu | gdk::Key::MenuKB => Some(PanelKeyNavigation::ContextMenu),
                    gdk::Key::F10
                        if modifiers.contains(gdk::ModifierType::SHIFT_MASK)
                            && !modifiers.intersects(
                                gdk::ModifierType::CONTROL_MASK
                                    | gdk::ModifierType::ALT_MASK
                                    | gdk::ModifierType::SUPER_MASK,
                            ) =>
                    {
                        Some(PanelKeyNavigation::ContextMenu)
                    }
                    _ if key.to_unicode() == Some(' ') => {
                        quick_search.borrow_mut().clear();
                        emit_space();
                        return glib::Propagation::Stop;
                    }
                    _ if key.to_unicode() == Some('*')
                        && !modifiers.intersects(
                            gdk::ModifierType::CONTROL_MASK
                                | gdk::ModifierType::ALT_MASK
                                | gdk::ModifierType::SUPER_MASK,
                        ) =>
                    {
                        Some(PanelKeyNavigation::InvertMarks)
                    }
                    _ => None,
                };
                if let Some(action) = action {
                    quick_search.borrow_mut().clear();
                    on_nav(action);
                    return glib::Propagation::Stop;
                }

                if modifiers.intersects(
                    gdk::ModifierType::CONTROL_MASK
                        | gdk::ModifierType::ALT_MASK
                        | gdk::ModifierType::SUPER_MASK,
                ) {
                    return glib::Propagation::Proceed;
                }

                let mut buffer = quick_search.borrow_mut();
                buffer.expire_if_needed();

                if key == gdk::Key::Escape {
                    if buffer.query.is_empty() {
                        return glib::Propagation::Proceed;
                    }
                    buffer.clear();
                    drop(buffer);
                    let _ = on_quick_search("");
                    return glib::Propagation::Stop;
                }

                if key == gdk::Key::BackSpace {
                    if buffer.query.is_empty() {
                        return glib::Propagation::Proceed;
                    }
                    buffer.query.pop();
                    let query = buffer.query.clone();
                    if query.is_empty() {
                        buffer.clear();
                    } else {
                        buffer.last_input = Some(Instant::now());
                    }
                    drop(buffer);
                    let _ = on_quick_search(&query);
                    return glib::Propagation::Stop;
                }

                // Fallback when the IM context does not consume the key (common on Windows).
                if let Some(ch) = key_to_search_char(key) {
                    let mut next = buffer.query.clone();
                    next.push(ch);
                    drop(buffer);
                    if on_quick_search(&next) {
                        quick_search.borrow_mut().set_query(next);
                    }
                    return glib::Propagation::Stop;
                }

                glib::Propagation::Proceed
            });
        }

        self.column_view.add_controller(controller);

        {
            let im_context_for_focus = im_context.clone();
            self.column_view.connect_has_focus_notify(move |view| {
                if view.has_focus() {
                    im_context_for_focus.focus_in();
                } else {
                    im_context_for_focus.focus_out();
                }
            });
            if self.column_view.has_focus() {
                im_context.focus_in();
            }
        }
    }

    pub fn connect_root_changed<F>(&self, f: F)
    where
        F: Fn(RootSelectorChoice) + 'static,
    {
        let ignore_roots = Rc::clone(&self.ignore_roots);
        let sftp_item_index = Rc::clone(&self.sftp_item_index);
        let last_drive_index = Rc::clone(&self.last_drive_index);
        let root_dropdown = self.root_dropdown.clone();
        self.root_dropdown.connect_selected_notify(move |dropdown| {
            if ignore_roots.get() {
                return;
            }
            let selected = dropdown.selected();
            if selected == gtk::INVALID_LIST_POSITION {
                return;
            }
            if sftp_item_index.get() == Some(selected) {
                ignore_roots.set(true);
                if let Some(drive_index) = last_drive_index.get() {
                    root_dropdown.set_selected(drive_index);
                }
                ignore_roots.set(false);
                f(RootSelectorChoice::Sftp);
                return;
            }
            last_drive_index.set(Some(selected));
            f(RootSelectorChoice::Drive(selected as usize));
        });
    }

    pub fn connect_secondary_click<F>(&self, f: F)
    where
        F: Fn(Option<usize>, f64, f64) + 'static,
    {
        let root = self.root.clone();
        let column_view = self.column_view.clone();
        let gesture = gtk::GestureClick::new();
        gesture.set_button(gdk::BUTTON_SECONDARY);
        gesture.connect_pressed(move |_, _, x, y| {
            let clicked_position = column_view
                .pick(x, y, gtk::PickFlags::DEFAULT)
                .and_then(|widget| find_row_position(&widget, &column_view));
            let point = column_view
                .compute_point(&root, &gtk::graphene::Point::new(x as f32, y as f32))
                .unwrap_or_else(|| gtk::graphene::Point::new(x as f32, y as f32));
            f(clicked_position, point.x() as f64, point.y() as f64);
        });
        self.column_view.add_controller(gesture);
    }

    pub fn apply_sort(&self, column: SortColumn, direction: SortDirection) {
        let Some(binding) = self
            .columns
            .iter()
            .find(|binding| binding.sort_column == column)
        else {
            return;
        };
        let sort_type = match direction {
            SortDirection::Ascending => gtk::SortType::Ascending,
            SortDirection::Descending => gtk::SortType::Descending,
        };
        let was_ignoring_sort = self.ignore_sort.replace(true);
        self.column_view
            .sort_by_column(Some(&binding.column), sort_type);
        self.last_sort
            .borrow_mut()
            .replace((column, sort_type));
        self.ignore_sort.set(was_ignoring_sort);
    }

    pub fn connect_sort_changed<F>(&self, f: F)
    where
        F: Fn(SortColumn, gtk::SortType) + 'static,
    {
        let Some(sorter) = self
            .column_view
            .sorter()
            .and_then(|sorter| sorter.downcast::<gtk::ColumnViewSorter>().ok())
        else {
            return;
        };
        let columns = self.columns.clone();
        let ignore_sort = Rc::clone(&self.ignore_sort);
        let last_sort = Rc::clone(&self.last_sort);
        sorter.connect_changed(move |sorter, _| {
            if ignore_sort.get() {
                return;
            }
            let Some(primary_column) = sorter.primary_sort_column() else {
                return;
            };
            let direction = sorter.primary_sort_order();
            if let Some(binding) = columns
                .iter()
                .find(|binding| binding.column == primary_column)
            {
                let next_sort = (binding.sort_column, direction);
                if last_sort.borrow().as_ref() == Some(&next_sort) {
                    return;
                }
                last_sort.borrow_mut().replace(next_sort);
                f(binding.sort_column, direction);
            }
        });
    }
}

fn find_row_position(widget: &gtk::Widget, root: &gtk::ColumnView) -> Option<usize> {
    let mut current = Some(widget.clone());
    while let Some(candidate) = current {
        unsafe {
            if let Some(position) = candidate.data::<usize>(ROW_POSITION_DATA_KEY) {
                return Some(*position.as_ref());
            }
        }

        if candidate == *root.upcast_ref::<gtk::Widget>() {
            break;
        }
        current = candidate.parent();
    }
    None
}

fn push_quick_search_text<Q>(buffer: &RefCell<QuickSearchBuffer>, on_quick_search: &Q, text: &str)
where
    Q: Fn(&str) -> bool + ?Sized,
{
    let text = text.trim();
    if text.is_empty() {
        return;
    }

    let mut state = buffer.borrow_mut();
    state.expire_if_needed();
    let mut next = state.query.clone();
    next.push_str(text);
    drop(state);

    if on_quick_search(&next) {
        buffer.borrow_mut().set_query(next);
    }
}

fn key_to_search_char(key: gdk::Key) -> Option<char> {
    key.to_unicode()
        .filter(|ch| !ch.is_control() && !ch.is_whitespace() && *ch != '*')
}

fn restore_panel_key_input(
    column_view: &gtk::ColumnView,
    panel_im_context: &RefCell<Option<gtk::IMMulticontext>>,
    panel_key_controller: &RefCell<Option<EventControllerKey>>,
) {
    if let Some(im) = panel_im_context.borrow().as_ref() {
        im.set_client_widget(Some(column_view));
        if let Some(controller) = panel_key_controller.borrow().as_ref() {
            controller.set_im_context(Some(&im.clone().upcast::<gtk::IMContext>()));
        }
    }
}

fn start_inline_rename_with_retry<F>(
    row: FileRowObject,
    column_view: gtk::ColumnView,
    name: String,
    is_dir: bool,
    is_marked: bool,
    on_commit: Rc<RefCell<Option<F>>>,
    cancel_slot: Rc<RefCell<Option<Rc<dyn Fn()>>>>,
    active: Rc<Cell<bool>>,
    panel_im_context: Rc<RefCell<Option<gtk::IMMulticontext>>>,
    panel_key_controller: Rc<RefCell<Option<EventControllerKey>>>,
    attempts_left: u32,
) where
    F: FnOnce(String) + 'static,
{
    if let Some(container) = row.name_cell() {
        let Some(on_commit) = on_commit.borrow_mut().take() else {
            return;
        };
        let cancel = begin_inline_rename(
            &container,
            &name,
            is_dir,
            is_marked,
            on_commit,
            {
                let active = Rc::clone(&active);
                let cancel_slot = Rc::clone(&cancel_slot);
                let im = Rc::clone(&panel_im_context);
                let keys = Rc::clone(&panel_key_controller);
                let column_view = column_view.clone();
                move || {
                    active.set(false);
                    let _ = cancel_slot.borrow_mut().take();
                    // Move focus off the dying cell before list refresh/rebind.
                    column_view.grab_focus();
                    restore_panel_key_input(&column_view, &im, &keys);
                }
            },
        );
        *cancel_slot.borrow_mut() = Some(cancel);
        return;
    }

    if attempts_left == 0 {
        active.set(false);
        restore_panel_key_input(&column_view, &panel_im_context, &panel_key_controller);
        return;
    }

    glib::timeout_add_local_once(Duration::from_millis(16), move || {
        start_inline_rename_with_retry(
            row,
            column_view,
            name,
            is_dir,
            is_marked,
            on_commit,
            cancel_slot,
            active,
            panel_im_context,
            panel_key_controller,
            attempts_left - 1,
        );
    });
}

fn install_root_selector_escape(dropdown: &gtk::DropDown, column_view: &gtk::ColumnView) {
    let restore_panel_focus = {
        let column_view = column_view.clone();
        Rc::new(move || {
            let column_view = column_view.clone();
            glib::idle_add_local_once(move || {
                column_view.grab_focus();
            });
        })
    };

    let attach_escape = |widget: &gtk::Widget| {
        let controller = EventControllerKey::new();
        controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        let dropdown = dropdown.clone();
        controller.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape && close_dropdown_popup(&dropdown) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        widget.add_controller(controller);
    };

    attach_escape(dropdown.upcast_ref());

    let mut child = dropdown.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if let Ok(popover) = widget.downcast::<gtk::Popover>() {
            attach_escape(popover.upcast_ref());
            let restore_panel_focus = Rc::clone(&restore_panel_focus);
            popover.connect_closed(move |_| {
                restore_panel_focus();
            });
        }
    }
}

fn close_dropdown_popup(dropdown: &gtk::DropDown) -> bool {
    let mut closed = false;
    let mut child = dropdown.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if let Ok(button) = widget.clone().downcast::<gtk::ToggleButton>() {
            if button.is_active() {
                button.set_active(false);
                closed = true;
            }
        } else if let Ok(popover) = widget.downcast::<gtk::Popover>() {
            if popover.is_visible() {
                popover.popdown();
                closed = true;
            }
        }
    }
    closed
}

/// Defer scroll_to(FOCUS) until ColumnView has a real height. Early calls during
/// startup/paned layout leave unbound rows as a solid theme-colored rectangle.
fn schedule_column_view_focus(column_view: gtk::ColumnView, index: u32) {
    schedule_column_view_focus_attempt(column_view, index, 0);
}

fn schedule_column_view_focus_attempt(
    column_view: gtk::ColumnView,
    index: u32,
    attempt: u32,
) {
    const MAX_ATTEMPTS: u32 = 30;
    let run = {
        let column_view = column_view.clone();
        move || {
            if column_view.height() < 48 && attempt < MAX_ATTEMPTS {
                schedule_column_view_focus_attempt(column_view, index, attempt + 1);
                return;
            }
            column_view.scroll_to(index, None, gtk::ListScrollFlags::FOCUS, None);
            column_view.queue_draw();
        }
    };
    if attempt == 0 {
        glib::idle_add_local_once(run);
    } else {
        glib::timeout_add_local_once(Duration::from_millis(16), run);
    }
}
