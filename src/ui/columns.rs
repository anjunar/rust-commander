use std::cmp::Ordering as StdOrdering;

use gtk::prelude::*;

use crate::{domain::sorting::SortColumn, ui::file_row_object::FileRowObject};

#[derive(Clone)]
pub struct FileColumnBinding {
    pub column: gtk::ColumnViewColumn,
    pub sort_column: SortColumn,
}

pub fn append_file_columns(view: &gtk::ColumnView) -> Vec<FileColumnBinding> {
    let specs = [
        ("Name", SortColumn::Name, 320, true),
        ("Size", SortColumn::Size, 96, false),
        ("Type", SortColumn::Type, 92, false),
        ("Modified", SortColumn::Modified, 150, false),
        ("Attributes", SortColumn::Attributes, 96, false),
    ];

    specs
        .into_iter()
        .map(|(title, sort_column, width, expands)| {
            let factory = match sort_column {
                SortColumn::Name => name_factory().upcast::<gtk::ListItemFactory>(),
                _ => text_factory(sort_column).upcast::<gtk::ListItemFactory>(),
            };
            let column = gtk::ColumnViewColumn::new(Some(title), Some(factory));
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

fn name_factory() -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();

    factory.connect_setup(|_, object| {
        let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.set_margin_start(6);
        row.set_margin_end(6);
        row.set_margin_top(4);
        row.set_margin_bottom(4);

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

        if let Some(image) = container
            .first_child()
            .and_then(|child| child.downcast::<gtk::Image>().ok())
        {
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
            label.set_label(&row_object.name());
            if row_object.is_parent_link() {
                label.add_css_class("parent-link");
            } else {
                label.remove_css_class("parent-link");
            }
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
        label.set_margin_start(6);
        label.set_margin_end(6);
        label.set_margin_top(6);
        label.set_margin_bottom(6);
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

        label.set_label(&text_for_column(&row_object, column));
    });

    factory
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
            SortColumn::Type => a.type_label().cmp(&b.type_label()),
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
