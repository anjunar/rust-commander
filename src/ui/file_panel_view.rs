use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
    path::PathBuf,
    rc::Rc,
};

use gtk::{gdk, gio, glib, prelude::*};
use rust_i18n::t;

use crate::{
    application::ActivePanel,
    domain::{
        sorting::{SortColumn, SortDirection},
        Entry, PanelLocation, RootLocation,
    },
    ui::{
        columns::{begin_inline_rename, file_columns},
        dsl::{
            button, column_view, dropdown_list, foreach_in, hbox, scrolled, text, tr, vbox,
            ListProperty, Property, Table,
        },
        file_row_object::FileRowObject,
        shell::ShellEvents,
    },
};

#[path = "file_panel_view_helpers.rs"]
mod helpers;
#[path = "file_panel_view_input.rs"]
mod input;

use helpers::{install_root_selector_escape, schedule_column_view_focus};
use input::{install_panel_keys, PanelInput};

/// What a key means to a panel, as opposed to the text typed into it.
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

/// One entry of the tab strip.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PanelTab {
    pub title: String,
    pub active: bool,
}

type SortColumns = Rc<Vec<(gtk::ColumnViewColumn, SortColumn)>>;

/// One commander panel: tabs, drive picker, path, file table and footer.
///
/// The header and footer are bound to properties; the rows live in a
/// `gio::ListStore` that GTK virtualises. The selection model holds the
/// cursor, the marks live on the row objects. The panel reports what the user
/// does to [`ShellEvents`] and never decides anything itself.
#[derive(Clone)]
pub struct FilePanel {
    table: Table,
    columns: SortColumns,
    store: gio::ListStore,
    selection: gtk::SingleSelection,
    picker: gtk::DropDown,
    tabs: ListProperty<PanelTab>,
    path: Property<String>,
    footer: Property<String>,
    roots: ListProperty<String>,
    selected_root: Property<u32>,
    active: Property<bool>,
    enabled: Property<bool>,
    /// Set while the panel applies state, so the widgets' change signals are
    /// not mistaken for user input.
    applying: Rc<Applying>,
    /// The root the panel is on: the picker returns here after "SFTP…".
    last_root: Rc<Cell<Option<u32>>>,
    last_sort: Rc<RefCell<Option<(SortColumn, gtk::SortType)>>>,
    current_entries_path: Rc<RefCell<Option<PathBuf>>>,
    input: Rc<PanelInput>,
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
    let selection = gtk::SingleSelection::new(Some(store.clone()));
    selection.set_autoselect(true);
    selection.set_can_unselect(false);

    let tabs = ListProperty::default();
    let path = Property::new(String::new());
    let footer = Property::new(String::new());
    let roots = ListProperty::default();
    let selected_root = Property::new(gtk::INVALID_LIST_POSITION);
    let active = Property::new(false);
    let enabled = Property::new(true);
    let applying = Rc::new(Applying::default());
    let last_root = Rc::new(Cell::new(None));
    let last_sort = Rc::new(RefCell::new(None));
    let input = Rc::new(PanelInput::new());

    let mut parts = None;
    let mut picker = None;
    vbox(6, |root| {
        root.css("file-panel");
        root.css(match panel {
            ActivePanel::Left => "file-panel-left",
            ActivePanel::Right => "file-panel-right",
        });
        root.set_hexpand(true);
        root.set_vexpand(true);
        root.css_if("active-panel", &active);

        tab_strip(panel, &tabs, &events);

        hbox(8, |header| {
            header.css("path-row");
            header.set_vexpand(false);
            header.set_valign(gtk::Align::Start);
            picker = Some(
                dropdown_list(&roots, &selected_root, |picker| {
                    picker.set_width_request(110);
                    picker.set_enable_search(false);
                    picker.css("root-selector");
                    picker.bind_sensitive(&enabled);
                    let events = Rc::clone(&events);
                    picker.on_focus_change(move |focused| {
                        if focused {
                            events.panel_focused(panel);
                        }
                    });
                })
                .widget()
                .clone(),
            );
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
            parts = Some(file_table(
                panel, &selection, &applying, &last_sort, &input, &enabled, &events,
            ));
        });

        text(&footer, |label| {
            label.set_hexpand(true);
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            label.css("panel-footer");
        });

        // The user picked a drive, a remote root, or the SFTP entry at the end.
        {
            let applying = Rc::clone(&applying);
            let events = Rc::clone(&events);
            let roots = roots.clone();
            let selected = selected_root.clone();
            let last_root = Rc::clone(&last_root);
            root.add_disposable(selected_root.observe_without_initial(move |index| {
                if applying.roots.get() || *index == gtk::INVALID_LIST_POSITION {
                    return;
                }
                if *index as usize + 1 == roots.len() {
                    // The property is still notifying; it cannot be set from here.
                    let applying = Rc::clone(&applying);
                    let selected = selected.clone();
                    let last_root = last_root.get();
                    glib::idle_add_local_once(move || {
                        if let Some(index) = last_root {
                            applying.roots.set(true);
                            selected.set(index);
                            applying.roots.set(false);
                        }
                    });
                    events.connect_remote(panel);
                    return;
                }
                last_root.set(Some(*index));
                events.root_selected(panel, *index as usize);
            }));
        }

        // The label of the SFTP entry follows the locale like everything else.
        {
            let applying = Rc::clone(&applying);
            let roots = roots.clone();
            root.add_disposable(
                tr!("panel.root_sftp").observe_without_initial(move |label| {
                    if let Some(last) = roots.len().checked_sub(1) {
                        applying.roots.set(true);
                        roots.set(last, label.clone());
                        applying.roots.set(false);
                    }
                }),
            );
        }

        // The user moved the cursor.
        {
            let applying = Rc::clone(&applying);
            let events = Rc::clone(&events);
            let handler = selection.connect_selection_changed(move |selection, _, _| {
                if applying.selection.get() {
                    return;
                }
                let selected = selection.selected();
                if selected != gtk::INVALID_LIST_POSITION {
                    events.cursor_changed(panel, selected as usize);
                }
            });
            root.own_object_signal(&selection, handler);
        }
    });

    let (table, columns) = parts.expect("file table composed");
    let picker = picker.expect("root picker composed");
    install_root_selector_escape(&picker, table.widget());

    FilePanel {
        table,
        columns,
        store,
        selection,
        picker,
        tabs,
        path,
        footer,
        roots,
        selected_root,
        active,
        enabled,
        applying,
        last_root,
        last_sort,
        current_entries_path: Rc::new(RefCell::new(None)),
        input,
    }
}

/// One button per tab, then the one that opens a new tab.
fn tab_strip(panel: ActivePanel, tabs: &ListProperty<PanelTab>, events: &Rc<dyn ShellEvents>) {
    hbox(0, |bar| {
        bar.css("panel-tab-bar");
        bar.set_hexpand(true);
        bar.set_vexpand(false);
        bar.set_valign(gtk::Align::Start);

        {
            let events = Rc::clone(events);
            foreach_in(
                gtk::Orientation::Horizontal,
                0,
                tabs,
                move |tab: &PanelTab, index| {
                    let events = Rc::clone(&events);
                    button(tab.title.as_str(), |button| {
                        button.css("panel-tab");
                        if tab.active {
                            button.css("active");
                        }
                        button.set_has_frame(false);
                        button.set_can_focus(false);
                        button.set_hexpand(false);
                        button.set_tooltip_text(Some(&tab.title));
                        if let Some(label) = button.child().and_downcast::<gtk::Label>() {
                            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                            label.set_max_width_chars(24);
                        }
                        button.on_click(move || events.tab_activated(panel, index));
                    });
                },
            );
        }

        button("+", |add| {
            add.css("panel-tab-add");
            add.set_has_frame(false);
            add.set_can_focus(false);
            add.bind_tooltip(tr!("menu.new_tab"));
            let events = Rc::clone(events);
            add.on_click(move || events.tab_new(panel));
        });
    });
}

/// The file table and everything the user can do with it.
fn file_table(
    panel: ActivePanel,
    selection: &gtk::SingleSelection,
    applying: &Rc<Applying>,
    last_sort: &Rc<RefCell<Option<(SortColumn, gtk::SortType)>>>,
    input: &Rc<PanelInput>,
    enabled: &Property<bool>,
    events: &Rc<dyn ShellEvents>,
) -> (Table, SortColumns) {
    let mut sort_columns = None;
    let table = column_view(selection, |table| {
        table.set_hexpand(true);
        table.set_vexpand(true);
        table.set_show_row_separators(false);
        table.set_show_column_separators(false);
        table.set_single_click_activate(false);
        table.set_enable_rubberband(false);
        table.css("file-table");
        table.bind_sensitive(enabled);

        let columns: SortColumns = Rc::new(file_columns(table));
        sort_columns = Some(Rc::clone(&columns));

        install_panel_keys(table, panel, input, events);

        {
            let events = Rc::clone(events);
            table.on_row_activated(move |position| events.row_activated(panel, position as usize));
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
            let last_sort = Rc::clone(last_sort);
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
    });
    (table, sort_columns.expect("file columns declared"))
}

impl FilePanel {
    /// The widget menus opened from this panel are anchored on.
    pub fn table_widget(&self) -> gtk::Widget {
        self.table.widget().clone().upcast()
    }

    pub fn set_active(&self, active: bool) {
        self.active.set(active);
    }

    pub fn set_path(&self, label: &str) {
        self.path.set(label.to_owned());
    }

    pub fn set_footer(&self, label: &str) {
        self.footer.set(label.to_owned());
    }

    pub fn set_tabs(&self, titles: Vec<String>, active: usize) {
        let tabs: Vec<PanelTab> = titles
            .into_iter()
            .enumerate()
            .map(|(index, title)| PanelTab {
                title,
                active: index == active,
            })
            .collect();
        if self.tabs.get() != tabs {
            self.tabs.replace_all(tabs);
        }
    }

    /// Only the loading panel is gated; the other one stays usable.
    pub fn set_interaction_enabled(&self, enabled: bool) {
        self.enabled.set(enabled);
    }

    pub fn grab_focus(&self) {
        self.table.grab_focus();
        let selected = self.selection.selected();
        if selected != gtk::INVALID_LIST_POSITION {
            schedule_column_view_focus(self.table.widget().clone(), selected);
        }
    }

    pub fn refresh_list_after_resize(&self) {
        let view = self.table.widget();
        view.queue_resize();
        view.queue_draw();
        let selected = self.selection.selected();
        if selected != gtk::INVALID_LIST_POSITION {
            schedule_column_view_focus(view.clone(), selected);
        }
    }

    pub fn open_root_selector(&self) {
        self.picker.grab_focus();
        self.picker.emit_activate();
    }

    pub fn set_roots(&self, roots: &[RootLocation], selected_index: Option<usize>) {
        self.applying.roots.set(true);
        let mut labels: Vec<String> = roots.iter().map(|root| root.label.clone()).collect();
        labels.push(t!("panel.root_sftp").into_owned());
        self.roots.replace_all(labels);

        let index = selected_index
            .filter(|index| *index < roots.len())
            .map(|index| index as u32)
            .or_else(|| {
                self.last_root
                    .get()
                    .filter(|index| (*index as usize) < roots.len())
            })
            .or_else(|| (!roots.is_empty()).then_some(0));
        if let Some(index) = index {
            self.selected_root.set_always(index);
            self.last_root.set(Some(index));
        }
        self.applying.roots.set(false);
    }

    /// Shows `column` as the sorted one, without reporting it back as a user action.
    pub fn apply_sort(&self, column: SortColumn, direction: SortDirection) {
        let Some((view_column, _)) = self.columns.iter().find(|(_, known)| *known == column) else {
            return;
        };
        let order = match direction {
            SortDirection::Ascending => gtk::SortType::Ascending,
            SortDirection::Descending => gtk::SortType::Descending,
        };
        let was_applying = self.applying.sort.replace(true);
        self.table.sort_by_column(Some(view_column), order);
        self.last_sort.replace(Some((column, order)));
        self.applying.sort.set(was_applying);
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
            self.input.clear_quick_search();
        }

        let was_applying_sort = self.applying.sort.replace(true);
        self.applying.selection.set(true);
        self.sync_store(location, entries, size_labels);
        self.apply_marks(&marks);

        let cursor = cursor.filter(|index| *index < entries.len());
        self.selection.set_selected(
            cursor
                .map(|index| index as u32)
                .unwrap_or(gtk::INVALID_LIST_POSITION),
        );
        self.applying.selection.set(false);
        self.applying.sort.set(was_applying_sort);

        // Keyboard navigation continues from the cursor, not from row 0.
        if let Some(index) = cursor {
            schedule_column_view_focus(self.table.widget().clone(), index as u32);
        }
    }

    /// Moves the cursor and restyles the marks without rebuilding the list.
    pub fn apply_cursor_and_marks(&self, cursor: Option<usize>, marks: &BTreeSet<usize>) {
        self.apply_marks(marks);
        let rows = self.store.n_items() as usize;
        if let Some(index) = cursor.filter(|index| *index < rows) {
            let index = index as u32;
            if self.selection.selected() != index {
                self.applying.selection.set(true);
                self.selection.set_selected(index);
                self.applying.selection.set(false);
                schedule_column_view_focus(self.table.widget().clone(), index);
            }
        }
    }

    pub fn update_row_size(&self, index: usize, size_label: &str) {
        let Some(row) = self.row_at(index) else {
            return;
        };
        if row.size() != size_label {
            row.set_size_label(size_label);
            self.table.refresh_row(index as u32);
        }
    }

    /// Resolves the icons again, for the rows on screen now and the rest on demand.
    pub fn reload_icons(&self) {
        for index in 0..self.store.n_items() as usize {
            if let Some(row) = self.row_at(index) {
                row.invalidate_icon();
            }
        }
        self.table.refresh_rows();
    }

    /// Replaces the name of row `index` with an entry until the user is done.
    pub fn begin_inline_rename<F>(&self, index: usize, name: &str, is_dir: bool, on_commit: F)
    where
        F: FnOnce(String) + 'static,
    {
        self.cancel_inline_rename();

        let Some(row) = self.row_at(index) else {
            return;
        };
        self.input.suspend();

        let is_marked = row.is_marked();
        let name = name.to_string();
        // Not FOCUS: that would pull the focus back out of the entry.
        self.table
            .scroll_to(index as u32, None, gtk::ListScrollFlags::SELECT, None);

        let input = Rc::clone(&self.input);
        let view = self.table.widget().clone();
        glib::idle_add_local_once(move || {
            start_inline_rename(
                row,
                view,
                name,
                is_dir,
                is_marked,
                Rc::new(RefCell::new(Some(on_commit))),
                input,
                10,
            );
        });
    }

    pub fn cancel_inline_rename(&self) {
        // The entry has to go before the panel's input method comes back:
        // the other way round aborts on Windows.
        if let Some(cancel) = self.input.take_rename_cancel() {
            cancel();
        }
        self.input.resume();
    }

    fn apply_marks(&self, marks: &BTreeSet<usize>) {
        for index in 0..self.store.n_items() as usize {
            let Some(row) = self.row_at(index) else {
                continue;
            };
            let marked = marks.contains(&index);
            if row.is_marked() != marked {
                row.set_marked(marked);
                self.table.refresh_row(index as u32);
            }
        }
    }

    /// Brings the store in line with `entries`, touching only the rows that changed.
    fn sync_store(&self, location: &PanelLocation, entries: &[Entry], size_labels: &[String]) {
        let size_label = |index: usize| size_labels.get(index).map_or("-", String::as_str);
        let mut current_entries_path = self.current_entries_path.borrow_mut();
        let location_label = location.display_label();
        let path_changed =
            current_entries_path.as_deref() != Some(std::path::Path::new(&location_label));

        // Row-by-row syncing is quadratic; a big list is cheaper to replace.
        if path_changed || entries.len() > 500 {
            let rows: Vec<FileRowObject> = entries
                .iter()
                .enumerate()
                .map(|(index, entry)| {
                    FileRowObject::new_with_size_label(location, entry, size_label(index))
                })
                .collect();
            self.store.splice(0, self.store.n_items(), &rows);
            *current_entries_path = Some(PathBuf::from(location_label));
            return;
        }

        let mut index = 0usize;
        while index < entries.len() {
            let entry = &entries[index];
            let label = size_label(index);
            let Some(row) = self.row_at(index) else {
                self.store.insert(
                    index as u32,
                    &FileRowObject::new_with_size_label(location, entry, label),
                );
                index += 1;
                continue;
            };

            if row.matches_entry(location, entry) {
                if row.size() != label {
                    row.set_size_label(label);
                    self.table.refresh_row(index as u32);
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
                &FileRowObject::new_with_size_label(location, entry, label),
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
}

/// Opens the editor once GTK has bound a cell for the row, which can take a
/// few frames after scrolling it into view.
#[allow(clippy::too_many_arguments)]
fn start_inline_rename<F>(
    row: FileRowObject,
    view: gtk::ColumnView,
    name: String,
    is_dir: bool,
    is_marked: bool,
    on_commit: Rc<RefCell<Option<F>>>,
    input: Rc<PanelInput>,
    attempts_left: u32,
) where
    F: FnOnce(String) + 'static,
{
    if let Some(cell) = row.name_cell() {
        let Some(on_commit) = on_commit.borrow_mut().take() else {
            return;
        };
        let cancel = begin_inline_rename(&cell, &name, is_dir, is_marked, on_commit, {
            let input = Rc::clone(&input);
            let view = view.clone();
            move || {
                input.take_rename_cancel();
                // Off the dying cell before the list refreshes and rebinds it.
                view.grab_focus();
                input.resume();
            }
        });
        input.set_rename_cancel(cancel);
        return;
    }

    if attempts_left == 0 {
        input.resume();
        return;
    }

    glib::timeout_add_local_once(std::time::Duration::from_millis(16), move || {
        start_inline_rename(
            row,
            view,
            name,
            is_dir,
            is_marked,
            on_commit,
            input,
            attempts_left - 1,
        );
    });
}
