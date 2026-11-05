use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use gtk::{gdk, glib, pango, prelude::*};
use rust_i18n::t;

use crate::{
    domain::sorting::SortColumn,
    ui::{
        dsl::{hbox, localized, mount, sorter_by, text, ReadOnly, Table},
        file_row_object::FileRowObject,
    },
};

/// Bright red for marked rows (#ff4d4d), as 16-bit Pango channels.
const MARKED_FG: (u16, u16, u16) = (0xffff, 0x4d4d, 0x4d4d);

/// Declares the file columns on `table` and returns which column sorts by what.
pub(crate) fn file_columns(table: &Table) -> Vec<(gtk::ColumnViewColumn, SortColumn)> {
    let attributes_width = if cfg!(target_os = "windows") { 96 } else { 128 };
    let name = name_column(table);
    configure(&name, SortColumn::Name, 320, true);

    let mut columns = vec![(name, SortColumn::Name)];
    for (sort_column, width) in [
        (SortColumn::Size, 96),
        (SortColumn::Type, 92),
        (SortColumn::Modified, 150),
        (SortColumn::Attributes, attributes_width),
    ] {
        let column = text_column(table, sort_column);
        configure(&column, sort_column, width, false);
        columns.push((column, sort_column));
    }
    columns
}

/// The column header text, following the locale.
pub(crate) fn column_title(column: SortColumn) -> ReadOnly<String> {
    localized(move || match column {
        SortColumn::Name => t!("column.name").into_owned(),
        SortColumn::Size => t!("column.size").into_owned(),
        SortColumn::Type => t!("column.type").into_owned(),
        SortColumn::Modified => t!("column.modified").into_owned(),
        SortColumn::Attributes => t!("column.attributes").into_owned(),
    })
}

fn configure(column: &gtk::ColumnViewColumn, sort: SortColumn, width: i32, expand: bool) {
    column.set_resizable(true);
    column.set_fixed_width(width);
    column.set_expand(expand);
    column.set_sorter(Some(&sorter_by(move |a: &FileRowObject, b| match sort {
        SortColumn::Name => a.name().to_lowercase().cmp(&b.name().to_lowercase()),
        SortColumn::Size => a.size_bytes().cmp(&b.size_bytes()),
        SortColumn::Type => a
            .type_sort_key()
            .cmp(&b.type_sort_key())
            .then_with(|| a.name().to_lowercase().cmp(&b.name().to_lowercase())),
        SortColumn::Modified => a.modified().cmp(&b.modified()),
        SortColumn::Attributes => a.attributes().cmp(&b.attributes()),
    })));
}

/// The widgets of one name cell. An inline rename puts an entry after the
/// label and hides the label until it ends.
struct NameCell {
    row: gtk::Box,
    image: gtk::Image,
    label: gtk::Label,
}

/// Icon and name; the parent link is set apart.
fn name_column(table: &Table) -> gtk::ColumnViewColumn {
    table.column_with(
        column_title(SortColumn::Name),
        || {
            let mut parts = None;
            let row = hbox(6, |row| {
                row.set_margin_start(4);
                row.set_margin_end(4);
                row.set_margin_top(1);
                row.set_margin_bottom(1);
                let image = mount(gtk::Image::new(), |image| image.set_pixel_size(16));
                let label = text("", |label| {
                    label.set_hexpand(true);
                    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                });
                parts = Some((image.widget().clone(), label.widget().clone()));
            });
            let (image, label) = parts.expect("name cell composed");
            NameCell {
                row: row.widget().clone(),
                image,
                label,
            }
        },
        |cell: &NameCell, row: &FileRowObject, _| {
            row.set_name_cell(Some(&cell.row));
            if name_editor(&cell.row).is_some() {
                return;
            }

            row.ensure_icon();
            if let Some(paintable) = row.icon_paintable() {
                cell.image.set_icon_name(None);
                cell.image.set_paintable(Some(&paintable));
            } else {
                cell.image
                    .set_paintable(Option::<&gtk::gdk::Paintable>::None);
                cell.image.set_icon_name(Some(&row.icon_name()));
            }
            if row.is_parent_link() {
                cell.label.add_css_class("parent-link");
            } else {
                cell.label.remove_css_class("parent-link");
            }
            show_row_text(&cell.label, &row.name(), row.is_marked());
        },
        |cell: &NameCell, row: &FileRowObject| {
            row.set_name_cell(None);
            // A rename must not survive into the row this cell shows next.
            if let Some(entry) = name_editor(&cell.row) {
                cell.row.remove(&entry);
                cell.label.set_visible(true);
            }
        },
    )
}

/// One text value per row; sizes are right-aligned, the rest starts under
/// its column title.
fn text_column(table: &Table, column: SortColumn) -> gtk::ColumnViewColumn {
    table.column_with(
        column_title(column),
        move || {
            text("", |label| {
                label.set_margin_start(4);
                label.set_margin_end(4);
                label.set_margin_top(1);
                label.set_margin_bottom(1);
                label.set_xalign(match column {
                    SortColumn::Size => 1.0,
                    _ => 0.0,
                });
                label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            })
            .widget()
            .clone()
        },
        move |label: &gtk::Label, row: &FileRowObject, _| {
            let value = match column {
                SortColumn::Name => row.name(),
                SortColumn::Size => row.size(),
                SortColumn::Type => row.type_label(),
                SortColumn::Modified => row.modified(),
                SortColumn::Attributes => row.attributes(),
            };
            show_row_text(label, &value, row.is_marked());
        },
        |_, _| {},
    )
}

fn show_row_text(label: &gtk::Label, value: &str, marked: bool) {
    if marked {
        label.add_css_class("marked");
        let escaped = glib::markup_escape_text(value);
        // Markup colour cannot be overridden by the row styles of the column view.
        label.set_markup(&format!(
            "<span foreground=\"#ff4d4d\" weight=\"bold\">{escaped}</span>"
        ));
        let attributes = pango::AttrList::new();
        let (red, green, blue) = MARKED_FG;
        attributes.insert(pango::AttrColor::new_foreground(red, green, blue));
        label.set_attributes(Some(&attributes));
    } else {
        label.remove_css_class("marked");
        label.set_attributes(None::<&pango::AttrList>);
        label.set_text(value);
    }
}

fn name_label(cell: &gtk::Box) -> Option<gtk::Label> {
    cell.first_child()?.next_sibling()?.downcast().ok()
}

fn name_editor(cell: &gtk::Box) -> Option<gtk::Entry> {
    cell.last_child()?.downcast().ok()
}

/// Starts an in-place rename in a name cell. Returns a synchronous cancel.
pub(crate) fn begin_inline_rename<F, G>(
    cell: &gtk::Box,
    original_name: &str,
    is_dir: bool,
    is_marked: bool,
    on_commit: F,
    on_finished: G,
) -> Rc<dyn Fn()>
where
    F: FnOnce(String) + 'static,
    G: FnOnce() + 'static,
{
    if name_editor(cell).is_some() {
        return Rc::new(|| {});
    }
    let Some(label) = name_label(cell) else {
        return Rc::new(|| {});
    };

    let original_name = original_name.to_string();
    // Keep the row height and width stable: an entry is taller and grows with its text.
    let label_height = label.height();
    label.set_visible(false);

    let entry = gtk::Entry::new();
    entry.add_css_class("inline-rename-entry");
    entry.set_text(&original_name);
    entry.set_hexpand(true);
    entry.set_halign(gtk::Align::Fill);
    entry.set_valign(gtk::Align::Center);
    // The natural width must not follow the file name; hexpand fills the cell.
    entry.set_width_chars(1);
    entry.set_max_width_chars(1);
    if label_height > 0 {
        entry.set_size_request(-1, label_height);
    }
    cell.append(&entry);

    let callbacks = Rc::new(RefCell::new(Some((
        Box::new(on_commit) as Box<dyn FnOnce(String)>,
        Box::new(on_finished) as Box<dyn FnOnce()>,
    ))));
    let finished = Rc::new(Cell::new(false));
    // Focus loss counts only once the grab settled; the column view's focus
    // churn would otherwise commit at once and look like F2 did nothing.
    let armed = Rc::new(Cell::new(false));

    let finish = {
        let cell = cell.clone();
        let entry = entry.clone();
        let label = label.clone();
        let callbacks = Rc::clone(&callbacks);
        let finished = Rc::clone(&finished);
        let armed = Rc::clone(&armed);
        let original_name = original_name.clone();
        Rc::new(move |commit: bool| {
            if finished.replace(true) {
                return;
            }
            armed.set(false);
            let Some((on_commit, on_finished)) = callbacks.borrow_mut().take() else {
                return;
            };
            let value = entry.text().to_string();
            let shown = if commit && !value.trim().is_empty() {
                value.trim()
            } else {
                original_name.as_str()
            };
            if entry.parent().as_ref() == Some(cell.upcast_ref()) {
                cell.remove(&entry);
                show_row_text(&label, shown, is_marked);
                label.set_visible(true);
            }

            // Give the keys back to the panel before the rename touches the disk.
            on_finished();
            if commit {
                glib::idle_add_local_once(move || on_commit(value));
            }
        }) as Rc<dyn Fn(bool)>
    };

    // Destroying the entry inside its own handlers aborts on Windows GTK.
    let schedule_finish = {
        let finish = Rc::clone(&finish);
        let finished = Rc::clone(&finished);
        Rc::new(move |commit: bool| {
            if finished.get() {
                return;
            }
            let finish = Rc::clone(&finish);
            glib::idle_add_local_once(move || finish(commit));
        }) as Rc<dyn Fn(bool)>
    };

    {
        let schedule_finish = Rc::clone(&schedule_finish);
        entry.connect_activate(move |_| schedule_finish(true));
    }
    {
        let schedule_finish = Rc::clone(&schedule_finish);
        let key = gtk::EventControllerKey::new();
        key.set_propagation_phase(gtk::PropagationPhase::Capture);
        key.connect_key_pressed(move |_, keyval, _, _| {
            if keyval == gdk::Key::Escape {
                schedule_finish(false);
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        entry.add_controller(key);
    }
    {
        let schedule_finish = Rc::clone(&schedule_finish);
        let armed = Rc::clone(&armed);
        let finished = Rc::clone(&finished);
        entry.connect_has_focus_notify(move |entry| {
            if armed.get() && !finished.get() && !entry.has_focus() {
                schedule_finish(true);
            }
        });
    }

    glib::idle_add_local_once({
        let entry = entry.clone();
        let original_name = original_name.clone();
        let armed = Rc::clone(&armed);
        let finished = Rc::clone(&finished);
        move || {
            if finished.get() {
                return;
            }
            entry.grab_focus();
            select_rename_region(&entry, &original_name, is_dir);
            glib::idle_add_local_once(move || {
                if finished.get() {
                    return;
                }
                if !entry.has_focus() {
                    entry.grab_focus();
                    select_rename_region(&entry, &original_name, is_dir);
                }
                armed.set(true);
            });
        }
    });

    // A cancel from outside must finish before the list is rebuilt.
    Rc::new(move || finish(false))
}

fn select_rename_region(entry: &gtk::Entry, name: &str, is_dir: bool) {
    if is_dir {
        entry.select_region(0, -1);
        return;
    }
    // GTK counts characters, not bytes.
    if let Some(dot_bytes) = name.rfind('.') {
        let dot_chars = name[..dot_bytes].chars().count();
        if dot_chars > 0 {
            entry.select_region(0, dot_chars as i32);
            return;
        }
    }
    entry.select_region(0, -1);
}
