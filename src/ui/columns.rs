use std::{
    cell::{Cell, RefCell},
    cmp::Ordering as StdOrdering,
    rc::Rc,
};

use gtk::{gdk, glib, pango, prelude::*, EventControllerKey};
use rust_i18n::t;

use crate::{domain::sorting::SortColumn, ui::file_row_object::FileRowObject};

/// Bright red for marked rows (#ff4d4d), as 16-bit Pango channels.
const MARKED_FG: (u16, u16, u16) = (0xffff, 0x4d4d, 0x4d4d);

pub const ROW_POSITION_DATA_KEY: &str = "rcommander-row-position";
const ROW_COLUMN_DATA_KEY: &str = "rcommander-row-column";
const ROW_ALT_CLASS: &str = "row-alt";

#[derive(Clone)]
pub struct FileColumnBinding {
    pub column: gtk::ColumnViewColumn,
    pub sort_column: SortColumn,
}

/// Refresh visible cell widgets for a row after in-place data changes (mark/size).
/// Avoids ListStore remove/insert, which resets SingleSelection to 0.
pub fn refresh_row_visuals(root: &impl IsA<gtk::Widget>, position: usize, row: &FileRowObject) {
    refresh_widget_tree(root.as_ref(), position, row);
}

fn refresh_widget_tree(widget: &gtk::Widget, position: usize, row: &FileRowObject) {
    let matches_position = unsafe {
        widget
            .data::<usize>(ROW_POSITION_DATA_KEY)
            .is_some_and(|pos| *pos.as_ref() == position)
    };

    if matches_position {
        let column = unsafe {
            widget
                .data::<SortColumn>(ROW_COLUMN_DATA_KEY)
                .map(|col| *col.as_ref())
        };

        if let Some(column) = column {
            if let Some(label) = widget.downcast_ref::<gtk::Label>() {
                apply_marked_style(label, &text_for_column(row, column), row.is_marked());
            } else if let Some(container) = widget.downcast_ref::<gtk::Box>() {
                if column == SortColumn::Name {
                    apply_name_cell_visuals(container, row);
                } else if let Some(label) = container
                    .last_child()
                    .and_then(|child| child.downcast::<gtk::Label>().ok())
                {
                    apply_marked_style(&label, &text_for_column(row, column), row.is_marked());
                }
            }
        }
    }

    let mut child = widget.first_child();
    while let Some(current) = child {
        refresh_widget_tree(&current, position, row);
        child = current.next_sibling();
    }
}

fn name_cell_is_editing(container: &gtk::Box) -> bool {
    container
        .last_child()
        .is_some_and(|child| child.is::<gtk::Entry>())
}

fn make_name_label(text: &str, is_marked: bool, is_parent_link: bool) -> gtk::Label {
    let label = gtk::Label::new(None);
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    if is_parent_link {
        label.add_css_class("parent-link");
    }
    apply_marked_style(&label, text, is_marked);
    label
}

/// Starts in-place rename in a name cell. Returns a synchronous cancel callback.
pub fn begin_inline_rename<F, G>(
    container: &gtk::Box,
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
    if name_cell_is_editing(container) {
        return Rc::new(|| {});
    }

    let Some(label) = container
        .last_child()
        .and_then(|child| child.downcast::<gtk::Label>().ok())
    else {
        return Rc::new(|| {});
    };

    let original_name = original_name.to_string();
    let is_parent_link = label.has_css_class("parent-link");
    // Keep the row height/width stable: Entry's defaults are taller and grow with text.
    let label_height = label.height();
    container.remove(&label);

    let entry = gtk::Entry::new();
    entry.add_css_class("inline-rename-entry");
    entry.set_text(&original_name);
    entry.set_hexpand(true);
    entry.set_halign(gtk::Align::Fill);
    entry.set_valign(gtk::Align::Center);
    // Natural width must not follow the filename; hexpand fills the name cell.
    entry.set_width_chars(1);
    entry.set_max_width_chars(1);
    if label_height > 0 {
        entry.set_size_request(-1, label_height);
    }
    container.append(&entry);

    let callbacks = Rc::new(RefCell::new(Some((
        Box::new(on_commit) as Box<dyn FnOnce(String)>,
        Box::new(on_finished) as Box<dyn FnOnce()>,
    ))));
    let finished = Rc::new(Cell::new(false));
    // Ignore focus-loss until grab_focus settles; otherwise ColumnView focus churn
    // immediately commits and looks like F2 did nothing.
    let armed = Rc::new(Cell::new(false));

    let finish = {
        let container = container.clone();
        let entry = entry.clone();
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
            let text = entry.text().to_string();
            let label_text = if commit {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    original_name.as_str()
                } else {
                    trimmed
                }
            } else {
                original_name.as_str()
            };
            if entry.parent().as_ref() == Some(container.upcast_ref()) {
                container.remove(&entry);
                container.append(&make_name_label(label_text, is_marked, is_parent_link));
            }

            // End panel key suspension before FS rename / panel refresh.
            on_finished();
            if commit {
                glib::idle_add_local_once(move || on_commit(text));
            }
        }) as Rc<dyn Fn(bool)>
    };

    // Destroying Entry inside its own activate/focus/key handlers aborts on Windows GTK.
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
        let key = EventControllerKey::new();
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

    // External cancel (set_entries / watcher) must finish synchronously before list rebuild.
    Rc::new({
        let finish = Rc::clone(&finish);
        move || finish(false)
    })
}

fn select_rename_region(entry: &gtk::Entry, name: &str, is_dir: bool) {
    if is_dir {
        entry.select_region(0, -1);
        return;
    }
    // GTK uses character offsets, not bytes.
    if let Some(dot_bytes) = name.rfind('.') {
        let dot_chars = name[..dot_bytes].chars().count();
        if dot_chars > 0 {
            entry.select_region(0, dot_chars as i32);
            return;
        }
    }
    entry.select_region(0, -1);
}

pub fn append_file_columns(view: &gtk::ColumnView) -> Vec<FileColumnBinding> {
    let attributes_width = if cfg!(target_os = "windows") { 96 } else { 128 };
    let specs = [
        (column_title(SortColumn::Name), SortColumn::Name, 320, true),
        (column_title(SortColumn::Size), SortColumn::Size, 96, false),
        (column_title(SortColumn::Type), SortColumn::Type, 92, false),
        (
            column_title(SortColumn::Modified),
            SortColumn::Modified,
            150,
            false,
        ),
        (
            column_title(SortColumn::Attributes),
            SortColumn::Attributes,
            attributes_width,
            false,
        ),
    ];

    specs
        .into_iter()
        .map(|(title, sort_column, width, expands)| {
            let factory = match sort_column {
                SortColumn::Name => name_factory().upcast::<gtk::ListItemFactory>(),
                _ => text_factory(sort_column).upcast::<gtk::ListItemFactory>(),
            };
            let column = gtk::ColumnViewColumn::new(Some(&title), Some(factory));
            column.set_resizable(true);
            column.set_fixed_width(width);
            column.set_expand(expands);
            column.set_sorter(Some(&sorter_for(sort_column)));
            view.append_column(&column);

            FileColumnBinding {
                column,
                sort_column,
            }
        })
        .collect()
}

pub fn column_title(column: SortColumn) -> String {
    match column {
        SortColumn::Name => t!("column.name").into_owned(),
        SortColumn::Size => t!("column.size").into_owned(),
        SortColumn::Type => t!("column.type").into_owned(),
        SortColumn::Modified => t!("column.modified").into_owned(),
        SortColumn::Attributes => t!("column.attributes").into_owned(),
    }
}

fn name_factory() -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();

    factory.connect_setup(|_, object| {
        let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        row.set_margin_start(4);
        row.set_margin_end(4);
        row.set_margin_top(1);
        row.set_margin_bottom(1);

        let image = gtk::Image::new();
        image.set_pixel_size(16);
        row.append(&image);

        let label = gtk::Label::new(None);
        label.set_xalign(0.0);
        label.set_hexpand(true);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        row.append(&label);

        list_item.set_child(Some(&row));
    });

    factory.connect_bind(|_, object| {
        let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(row_object) = row_object(list_item) else {
            return;
        };
        let Some(container) = list_item
            .child()
            .and_then(|child| child.downcast::<gtk::Box>().ok())
        else {
            return;
        };

        // Drop a recycled inline-rename Entry from a previous item.
        if let Some(entry) = container
            .last_child()
            .and_then(|child| child.downcast::<gtk::Entry>().ok())
        {
            container.remove(&entry);
            container.append(&make_name_label("", false, false));
        }

        if let Some(image) = container
            .first_child()
            .and_then(|child| child.downcast::<gtk::Image>().ok())
        {
            row_object.ensure_icon();
            if let Some(paintable) = row_object.icon_paintable() {
                image.set_icon_name(None);
                image.set_paintable(Some(&paintable));
            } else {
                image.set_paintable(Option::<&gtk::gdk::Paintable>::None);
                image.set_icon_name(Some(&row_object.icon_name()));
            }
        }

        if let Some(label) = container
            .last_child()
            .and_then(|child| child.downcast::<gtk::Label>().ok())
        {
            if row_object.is_parent_link() {
                label.add_css_class("parent-link");
            } else {
                label.remove_css_class("parent-link");
            }
            apply_marked_style(&label, &row_object.name(), row_object.is_marked());
        }

        row_object.set_name_cell(Some(&container));
        bind_row_meta(&container, list_item.position() as usize, SortColumn::Name);
    });

    factory.connect_unbind(|_, object| {
        let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        if let Some(row_object) = row_object(list_item) {
            row_object.set_name_cell(None);
        }
    });

    factory
}

fn text_factory(column: SortColumn) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();

    factory.connect_setup(move |_, object| {
        let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let label = gtk::Label::new(None);
        label.set_margin_start(4);
        label.set_margin_end(4);
        label.set_margin_top(1);
        label.set_margin_bottom(1);
        label.set_xalign(match column {
            SortColumn::Size => 1.0,
            _ => 0.5,
        });
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        list_item.set_child(Some(&label));
    });

    factory.connect_bind(move |_, object| {
        let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(row_object) = row_object(list_item) else {
            return;
        };
        let Some(label) = list_item
            .child()
            .and_then(|child| child.downcast::<gtk::Label>().ok())
        else {
            return;
        };

        apply_marked_style(
            &label,
            &text_for_column(&row_object, column),
            row_object.is_marked(),
        );
        bind_row_meta(&label, list_item.position() as usize, column);
    });

    factory
}

fn apply_name_cell_visuals(container: &gtk::Box, row: &FileRowObject) {
    if name_cell_is_editing(container) {
        return;
    }
    row.ensure_icon();
    if let Some(image) = container
        .first_child()
        .and_then(|child| child.downcast::<gtk::Image>().ok())
    {
        if let Some(paintable) = row.icon_paintable() {
            image.set_icon_name(None);
            image.set_paintable(Some(&paintable));
        } else {
            image.set_paintable(Option::<&gtk::gdk::Paintable>::None);
            image.set_icon_name(Some(&row.icon_name()));
        }
    }
    if let Some(label) = container
        .last_child()
        .and_then(|child| child.downcast::<gtk::Label>().ok())
    {
        if row.is_parent_link() {
            label.add_css_class("parent-link");
        } else {
            label.remove_css_class("parent-link");
        }
        apply_marked_style(&label, &row.name(), row.is_marked());
    }
}

fn apply_marked_style(label: &gtk::Label, text: &str, marked: bool) {
    if marked {
        label.add_css_class("marked");
        let escaped = glib::markup_escape_text(text);
        // Markup color cannot be overridden by ColumnView row inheritance.
        label.set_markup(&format!(
            "<span foreground=\"#ff4d4d\" weight=\"bold\">{escaped}</span>"
        ));
        let attrs = pango::AttrList::new();
        let (red, green, blue) = MARKED_FG;
        attrs.insert(pango::AttrColor::new_foreground(red, green, blue));
        label.set_attributes(Some(&attrs));
    } else {
        label.remove_css_class("marked");
        label.set_attributes(None::<&pango::AttrList>);
        label.set_text(text);
    }
}

fn bind_row_meta(widget: &impl IsA<gtk::Widget>, position: usize, column: SortColumn) {
    let widget = widget.as_ref();
    unsafe {
        widget.set_data(ROW_POSITION_DATA_KEY, position);
        widget.set_data(ROW_COLUMN_DATA_KEY, column);
    }
    // Model-index zebra — `:nth-child` flips when ColumnView recycles rows.
    if position % 2 == 1 {
        widget.add_css_class(ROW_ALT_CLASS);
    } else {
        widget.remove_css_class(ROW_ALT_CLASS);
    }
}

fn row_object(list_item: &gtk::ListItem) -> Option<FileRowObject> {
    list_item
        .item()
        .and_then(|item| item.downcast::<FileRowObject>().ok())
}

fn text_for_column(row: &FileRowObject, column: SortColumn) -> String {
    match column {
        SortColumn::Name => row.name(),
        SortColumn::Size => row.size(),
        SortColumn::Type => row.type_label(),
        SortColumn::Modified => row.modified(),
        SortColumn::Attributes => row.attributes(),
    }
}

fn sorter_for(column: SortColumn) -> gtk::CustomSorter {
    gtk::CustomSorter::new(move |a, b| {
        let Some(a) = a.downcast_ref::<FileRowObject>() else {
            return gtk::Ordering::Equal;
        };
        let Some(b) = b.downcast_ref::<FileRowObject>() else {
            return gtk::Ordering::Equal;
        };

        let ordering = match column {
            SortColumn::Name => a.name().to_lowercase().cmp(&b.name().to_lowercase()),
            SortColumn::Size => a.size_bytes().cmp(&b.size_bytes()),
            SortColumn::Type => crate::domain::sorting::file_extension(&a.name())
                .to_lowercase()
                .cmp(&crate::domain::sorting::file_extension(&b.name()).to_lowercase())
                .then_with(|| a.name().to_lowercase().cmp(&b.name().to_lowercase())),
            SortColumn::Modified => a.modified().cmp(&b.modified()),
            SortColumn::Attributes => a.attributes().cmp(&b.attributes()),
        };

        gtk_ordering(ordering)
    })
}

fn gtk_ordering(ordering: StdOrdering) -> gtk::Ordering {
    match ordering {
        StdOrdering::Less => gtk::Ordering::Smaller,
        StdOrdering::Equal => gtk::Ordering::Equal,
        StdOrdering::Greater => gtk::Ordering::Larger,
    }
}
