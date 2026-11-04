use gtk::prelude::*;
use rust_i18n::t;

use crate::{
    domain::sorting::SortColumn,
    ui::{
        dsl::{hbox, localized, mount, sorter_by, text, ReadOnly, Table},
        file_row_object::FileRowObject,
    },
};

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

/// Icon and name; the parent link is set apart.
fn name_column(table: &Table) -> gtk::ColumnViewColumn {
    table.column(
        column_title(SortColumn::Name),
        || {
            let mut cell = None;
            hbox(8, |row| {
                row.set_margin_start(6);
                row.set_margin_end(6);
                row.set_margin_top(4);
                row.set_margin_bottom(4);
                let image = mount(gtk::Image::new(), |image| image.set_pixel_size(16));
                let label = text("", |label| {
                    label.set_hexpand(true);
                    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                });
                cell = Some((image.widget().clone(), label.widget().clone()));
            });
            cell.expect("name cell composed")
        },
        |(image, label): &(gtk::Image, gtk::Label), row: &FileRowObject| {
            if let Some(paintable) = row.icon_paintable() {
                image.set_icon_name(None);
                image.set_paintable(Some(&paintable));
            } else {
                image.set_paintable(Option::<&gtk::gdk::Paintable>::None);
                image.set_icon_name(Some(&row.icon_name()));
            }
            label.set_label(&row.name());
            if row.is_parent_link() {
                label.add_css_class("parent-link");
            } else {
                label.remove_css_class("parent-link");
            }
        },
    )
}

/// One text value per row; sizes are right-aligned, the rest centred.
fn text_column(table: &Table, column: SortColumn) -> gtk::ColumnViewColumn {
    table.column(
        column_title(column),
        move || {
            text("", |label| {
                label.margin(6);
                label.set_xalign(match column {
                    SortColumn::Size => 1.0,
                    _ => 0.5,
                });
                label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            })
            .widget()
            .clone()
        },
        move |label: &gtk::Label, row: &FileRowObject| {
            label.set_label(&match column {
                SortColumn::Name => row.name(),
                SortColumn::Size => row.size(),
                SortColumn::Type => row.type_label(),
                SortColumn::Modified => row.modified(),
                SortColumn::Attributes => row.attributes(),
            });
        },
    )
}
