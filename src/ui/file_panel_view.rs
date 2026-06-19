use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
    path::PathBuf,
    rc::Rc,
};

use gtk::{gdk, EventControllerKey};
use gtk::{gio, glib, prelude::*};

use crate::{
    application::ActivePanel,
    domain::{sorting::SortColumn, Entry, PanelLocation, RootLocation},
    ui::{
        columns::{append_file_columns, FileColumnBinding, ROW_POSITION_DATA_KEY},
        file_row_object::FileRowObject,
    },
};

pub struct FilePanelView {
    pub panel: ActivePanel,
    pub root: gtk::Box,
    pub root_model: gtk::StringList,
    pub root_dropdown: gtk::DropDown,
    pub path_label: gtk::Label,
    pub store: gio::ListStore,
    pub selection: gtk::MultiSelection,
    pub column_view: gtk::ColumnView,
    pub columns: Vec<FileColumnBinding>,
    ignore_selection: Rc<Cell<bool>>,
    ignore_roots: Rc<Cell<bool>>,
    ignore_sort: Rc<Cell<bool>>,
    last_sort: Rc<RefCell<Option<(SortColumn, gtk::SortType)>>>,
    current_entries_path: Rc<RefCell<Option<PathBuf>>>,
}

impl FilePanelView {
    pub fn new(panel: ActivePanel) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
        root.set_hexpand(true);
        root.set_vexpand(true);
        root.add_css_class("file-panel");

        let path_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        path_row.add_css_class("path-row");

        let root_model = gtk::StringList::new(&[]);
        let root_dropdown = gtk::DropDown::new(Some(root_model.clone()), gtk::Expression::NONE);
        root_dropdown.set_width_request(110);
        root_dropdown.set_enable_search(true);
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
        let selection = gtk::MultiSelection::new(Some(store.clone()));
        let column_view = gtk::ColumnView::new(Some(selection.clone()));
        column_view.set_hexpand(true);
        column_view.set_vexpand(true);
        column_view.set_show_row_separators(false);
        column_view.set_show_column_separators(false);
        column_view.set_single_click_activate(false);
        column_view.set_enable_rubberband(true);
        column_view.add_css_class("file-table");

        let columns = append_file_columns(&column_view);

        let scrolled = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .child(&column_view)
            .build();
        scrolled.add_css_class("panel-scroller");
        root.append(&scrolled);

        Self {
            panel,
            root,
            root_model,
            root_dropdown,
            path_label,
            store,
            selection,
            column_view,
            columns,
            ignore_selection: Rc::new(Cell::new(false)),
            ignore_roots: Rc::new(Cell::new(false)),
            ignore_sort: Rc::new(Cell::new(false)),
            last_sort: Rc::new(RefCell::new(None)),
            current_entries_path: Rc::new(RefCell::new(None)),
        }
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

    pub fn set_entries(
        &self,
        location: &PanelLocation,
        entries: &[Entry],
        selected: BTreeSet<usize>,
    ) {
        let was_ignoring_sort = self.ignore_sort.replace(true);
        self.ignore_selection.set(true);
        let primary_selected = selected.iter().next().copied();
        self.sync_store(location, entries);

        self.selection.unselect_all();
        for index in selected {
            if index < entries.len() {
                self.selection.select_item(index as u32, false);
            }
        }
        self.ignore_selection.set(false);
        self.ignore_sort.set(was_ignoring_sort);

        if let Some(index) = primary_selected {
            let column_view = self.column_view.clone();
            glib::idle_add_local_once(move || {
                column_view.scroll_to(
                    index as u32,
                    None,
                    gtk::ListScrollFlags::SELECT | gtk::ListScrollFlags::FOCUS,
                    None,
                );
            });
        }
    }

    fn sync_store(&self, location: &PanelLocation, entries: &[Entry]) {
        let mut current_entries_path = self.current_entries_path.borrow_mut();
        let location_label = location.display_label();
        if current_entries_path.as_deref() != Some(std::path::Path::new(&location_label)) {
            self.store.remove_all();
            for entry in entries {
                self.store.append(&FileRowObject::new(location, entry));
            }
            *current_entries_path = Some(PathBuf::from(location_label));
            return;
        }

        let mut index = 0usize;

        while index < entries.len() {
            let entry = &entries[index];
            let Some(row) = self.row_at(index) else {
                self.store
                    .insert(index as u32, &FileRowObject::new(location, entry));
                index += 1;
                continue;
            };

            if row.matches_entry(location, entry) {
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
            self.store
                .insert(index as u32, &FileRowObject::new(location, entry));
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
        let labels = roots
            .iter()
            .map(|root| root.label.as_str())
            .collect::<Vec<_>>();
        self.root_model
            .splice(0, self.root_model.n_items(), labels.as_slice());
        if let Some(index) = selected_index {
            self.root_dropdown.set_selected(index as u32);
        }
        self.ignore_roots.set(false);
    }

    pub fn selected_indices(&self) -> Vec<usize> {
        (0..self.selection.n_items())
            .filter(|index| self.selection.is_selected(*index))
            .map(|index| index as usize)
            .collect()
    }

    pub fn grab_focus(&self) {
        self.column_view.grab_focus();
    }

    pub fn set_interaction_enabled(&self, enabled: bool) {
        self.root_dropdown.set_sensitive(enabled);
        self.column_view.set_sensitive(enabled);
    }

    pub fn connect_selection_changed<F>(&self, f: F)
    where
        F: Fn(Vec<usize>) + 'static,
    {
        let ignore_selection = Rc::clone(&self.ignore_selection);
        let selection = self.selection.clone();
        self.selection.connect_selection_changed(move |_, _, _| {
            if ignore_selection.get() {
                return;
            }
            let indices = (0..selection.n_items())
                .filter(|index| selection.is_selected(*index))
                .map(|index| index as usize)
                .collect::<Vec<_>>();
            f(indices);
        });
    }

    pub fn connect_activate<F>(&self, f: F)
    where
        F: Fn(usize) + 'static,
    {
        self.column_view.connect_activate(move |_, position| {
            f(position as usize);
        });
    }

    pub fn connect_open_key<F>(&self, f: F)
    where
        F: Fn() + 'static,
    {
        let controller = EventControllerKey::new();
        controller.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Return || key == gdk::Key::KP_Enter {
                f();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        self.column_view.add_controller(controller);
    }

    pub fn connect_root_changed<F>(&self, f: F)
    where
        F: Fn(usize) + 'static,
    {
        let ignore_roots = Rc::clone(&self.ignore_roots);
        self.root_dropdown.connect_selected_notify(move |dropdown| {
            if ignore_roots.get() {
                return;
            }
            let selected = dropdown.selected();
            if selected != gtk::INVALID_LIST_POSITION {
                f(selected as usize);
            }
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
