use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
    path::PathBuf,
    rc::Rc,
};

use gtk::{gdk, gio, glib, prelude::*};

use crate::{
    application::ActivePanel,
    domain::{
        sorting::{SortColumn, SortDirection},
        Entry, PanelLocation, RootLocation,
    },
    ui::{
        columns::file_columns,
        dsl::{
            button, column_view, dropdown_list, hbox, scrolled, text, vbox, ListProperty, Property,
            Table,
        },
        file_row_object::FileRowObject,
        shell::ShellEvents,
    },
};

/// One commander panel: drive picker, path, and the file table.
///
/// The header is bound to properties; the rows live in a `gio::ListStore`
/// that GTK virtualises. The panel reports what the user does to
/// [`ShellEvents`] and never decides anything itself.
#[derive(Clone)]
pub struct FilePanel {
    table: Table,
    store: gio::ListStore,
    selection: gtk::MultiSelection,
    path: Property<String>,
    roots: ListProperty<String>,
    selected_root: Property<u32>,
    active: Property<bool>,
    /// Set while the panel applies state, so the widgets' change signals are
    /// not mistaken for user input.
    applying: Rc<Applying>,
    current_entries_path: Rc<RefCell<Option<PathBuf>>>,
}

#[derive(Default)]
struct Applying {
    roots: Cell<bool>,
    selection: Cell<bool>,
    sort: Cell<bool>,
}

/// Composes a panel into the current parent.
pub(crate) fn file_panel(panel: ActivePanel, events: Rc<dyn ShellEvents>) -> FilePanel {
    let store = gio::ListStore::new::<FileRowObject>();
    let selection = gtk::MultiSelection::new(Some(store.clone()));
    let path = Property::new(String::new());
    let roots = ListProperty::default();
    let selected_root = Property::new(gtk::INVALID_LIST_POSITION);
    let active = Property::new(false);
    let applying = Rc::new(Applying::default());

    let mut table = None;
    vbox(6, |root| {
        root.css("file-panel");
        root.css(match panel {
            ActivePanel::Left => "file-panel-left",
            ActivePanel::Right => "file-panel-right",
        });
        root.set_hexpand(true);
        root.set_vexpand(true);
        root.css_if("active-panel", &active);

        hbox(8, |header| {
            header.css("path-row");
            dropdown_list(&roots, &selected_root, |picker| {
                picker.set_width_request(110);
                picker.set_enable_search(true);
                picker.css("root-selector");
                let events = Rc::clone(&events);
                picker.on_focus_change(move |focused| {
                    if focused {
                        events.panel_focused(panel);
                    }
                });
            });
            button("Connect", |connect| {
                connect.css("flat");
                let events = Rc::clone(&events);
                connect.on_click(move || events.connect_remote(panel));
            });
            text(&path, |label| {
                label.set_hexpand(true);
                label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
                label.css("path-label");
            });
        });

        scrolled(|scroller| {
            scroller.set_hexpand(true);
            scroller.set_vexpand(true);
            scroller.css("panel-scroller");
            table = Some(file_table(panel, &selection, &applying, &events));
        });

        // The user picked a drive or a remote root.
        {
            let applying = Rc::clone(&applying);
            let events = Rc::clone(&events);
            root.add_disposable(selected_root.observe_without_initial(move |index| {
                if !applying.roots.get() && *index != gtk::INVALID_LIST_POSITION {
                    events.root_selected(panel, *index as usize);
                }
            }));
        }

        // The user changed the selection.
        {
            let applying = Rc::clone(&applying);
            let events = Rc::clone(&events);
            let handler = selection.connect_selection_changed(move |selection, _, _| {
                if applying.selection.get() {
                    return;
                }
                let indices = (0..selection.n_items())
                    .filter(|index| selection.is_selected(*index))
                    .map(|index| index as usize)
                    .collect();
                events.selection_changed(panel, indices);
            });
            root.own_object_signal(&selection, handler);
        }
    });

    FilePanel {
        table: table.expect("file table composed"),
        store,
        selection,
        path,
        roots,
        selected_root,
        active,
        applying,
        current_entries_path: Rc::new(RefCell::new(None)),
    }
}

/// The file table and everything the user can do with it.
fn file_table(
    panel: ActivePanel,
    selection: &gtk::MultiSelection,
    applying: &Rc<Applying>,
    events: &Rc<dyn ShellEvents>,
) -> Table {
    column_view(selection, |table| {
        table.set_hexpand(true);
        table.set_vexpand(true);
        table.set_show_row_separators(false);
        table.set_show_column_separators(false);
        table.set_single_click_activate(false);
        table.set_enable_rubberband(true);
        table.css("file-table");

        let columns = file_columns(table);

        {
            let events = Rc::clone(events);
            table.on_row_activated(move |position| events.row_activated(panel, position as usize));
        }
        {
            let events = Rc::clone(events);
            table.on_key_pressed(move |key, _| {
                if key == gdk::Key::Return || key == gdk::Key::KP_Enter {
                    events.open_requested(panel);
                    true
                } else {
                    false
                }
            });
        }
        {
            let events = Rc::clone(events);
            table.on_pressed(gdk::BUTTON_PRIMARY, move |_, _| events.panel_focused(panel));
        }
        {
            let events = Rc::clone(events);
            table.on_focus_change(move |focused| {
                if focused {
                    events.panel_focused(panel);
                }
            });
        }
        {
            let events = Rc::clone(events);
            let anchor = table.clone();
            table.on_pressed(gdk::BUTTON_SECONDARY, move |x, y| {
                let row = anchor.row_at(x, y).map(|row| row as usize);
                let widget = anchor.widget().clone().upcast::<gtk::Widget>();
                events.context_menu(panel, row, widget, x, y);
            });
        }
        {
            let events = Rc::clone(events);
            let applying = Rc::clone(applying);
            let last_sort: RefCell<Option<(SortColumn, gtk::SortType)>> = RefCell::new(None);
            table.on_sort_changed(move |column, order| {
                if applying.sort.get() {
                    return;
                }
                let Some((_, sort_column)) = columns.iter().find(|(known, _)| known == column)
                else {
                    return;
                };
                let next = (*sort_column, order);
                if last_sort.borrow().as_ref() == Some(&next) {
                    return;
                }
                last_sort.replace(Some(next));
                let direction = match order {
                    gtk::SortType::Descending => SortDirection::Descending,
                    _ => SortDirection::Ascending,
                };
                events.sort_changed(panel, *sort_column, direction);
            });
        }
    })
}

impl FilePanel {
    pub fn set_active(&self, active: bool) {
        self.active.set(active);
    }

    pub fn set_path(&self, label: &str) {
        self.path.set(label.to_owned());
    }

    pub fn grab_focus(&self) {
        self.table.grab_focus();
    }

    pub fn set_roots(&self, roots: &[RootLocation], selected_index: Option<usize>) {
        self.applying.roots.set(true);
        self.roots
            .replace_all(roots.iter().map(|root| root.label.clone()).collect());
        if let Some(index) = selected_index {
            self.selected_root.set_always(index as u32);
        }
        self.applying.roots.set(false);
    }

    pub fn set_entries(
        &self,
        location: &PanelLocation,
        entries: &[Entry],
        selected: BTreeSet<usize>,
    ) {
        let was_applying_sort = self.applying.sort.replace(true);
        self.applying.selection.set(true);
        let primary_selected = selected.iter().next().copied();
        self.sync_store(location, entries);

        self.selection.unselect_all();
        for index in selected {
            if index < entries.len() {
                self.selection.select_item(index as u32, false);
            }
        }
        self.applying.selection.set(false);
        self.applying.sort.set(was_applying_sort);

        if let Some(index) = primary_selected {
            let view = self.table.widget().clone();
            glib::idle_add_local_once(move || {
                view.scroll_to(
                    index as u32,
                    None,
                    gtk::ListScrollFlags::SELECT | gtk::ListScrollFlags::FOCUS,
                    None,
                );
            });
        }
    }

    /// Brings the store in line with `entries`, touching only the rows that changed.
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
}
