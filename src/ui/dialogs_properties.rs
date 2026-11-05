use std::{borrow::Cow, fs, rc::Rc};

use gtk::prelude::*;
use rust_i18n::t;

use crate::{
    domain::{Entry, PanelLocation},
    fs::reader::{format_bytes, format_system_time},
    presentation,
    ui::dsl::{adopt, button, entry, hbox, mount, text, Ctx, Property},
};

use super::dialogs_base::{modal, present_and_focus, show_error, Once};

type PropertyRow = (Cow<'static, str>, String);

/// Show a read-only properties dialog for the selected panel entry.
pub fn show_properties(
    parent: &gtk::ApplicationWindow,
    entry: &Entry,
    location: &PanelLocation,
    calculated_dir_size: Option<u64>,
) {
    let (size, modified, attributes, created) =
        property_values(entry, location, calculated_dir_size);
    let mut rows = identity_rows(entry, location);
    rows.push((t!("dialog.properties_size"), size));
    if let Some(created) = created {
        rows.push((t!("dialog.properties_created"), created));
    }
    rows.push((t!("dialog.properties_modified"), modified));
    rows.push((t!("dialog.properties_attributes"), attributes));

    let (window, close) = modal(
        parent,
        &t!("dialog.properties_title"),
        (480, 320),
        |dialog| {
            dialog.content(|| {
                property_grid(&rows);
            });
            dialog.actions(|| {
                let close = dialog.close_button(t!("common.close"));
                close.css("suggested-action");
                close
            })
        },
    );

    present_and_focus(&window, close.widget());
}

/// SFTP properties with editable Unix permissions (octal + checkboxes).
pub fn show_remote_properties<F>(
    parent: &gtk::ApplicationWindow,
    entry: &Entry,
    location: &PanelLocation,
    calculated_dir_size: Option<u64>,
    on_apply_permissions: F,
) where
    F: FnOnce(u32) + 'static,
{
    let (size, modified, _, _) = property_values(entry, location, calculated_dir_size);
    let mut rows = identity_rows(entry, location);
    rows.push((t!("dialog.properties_size"), size));
    rows.push((t!("dialog.properties_modified"), modified));

    let form = Rc::new(PermissionsForm::new(parse_remote_mode(
        &entry.attributes,
        entry.is_dir,
    )));
    let on_apply = Once::new(on_apply_permissions);

    let (window, apply) = modal(
        parent,
        &t!("dialog.properties_title"),
        (520, 420),
        |dialog| {
            dialog.content(|| {
                property_grid(&rows);
                permissions_editor(&form);
            });
            dialog.actions(|| {
                dialog.close_button(t!("common.cancel"));
                button(t!("common.apply"), |button| {
                    button.css("suggested-action");
                    let form = Rc::clone(&form);
                    let parent = parent.clone();
                    let window = dialog.window().widget().clone();
                    let on_apply = on_apply.clone();
                    button.on_click(move || apply_permissions(&form, &parent, &window, &on_apply));
                })
            })
        },
    );
    window.set_default_widget(Some(apply.widget()));

    present_and_focus(&window, apply.widget());
}

/// The permissions being edited.
///
/// `octal` is the text of the entry and can be invalid while the user types.
/// `mode` is the last valid mode it held, which the checkboxes and the
/// symbolic text show. The text is the single way in: a checkbox writes it,
/// and it updates the mode.
struct PermissionsForm {
    octal: Property<String>,
    mode: Property<u32>,
}

impl PermissionsForm {
    fn new(mode: u32) -> Self {
        let mode = mode & 0o777;
        Self {
            octal: Property::new(format!("{mode:03o}")),
            mode: Property::new(mode),
        }
    }

    fn set_bit(&self, mask: u32, active: bool) {
        let mode = self.mode.get();
        let next = if active { mode | mask } else { mode & !mask };
        // A checkbox moved by the mode lands here too, with its bit already in
        // place. Writing the text then would happen inside the text's own
        // notification, and "0644" would not even survive the round trip.
        if next != mode {
            self.octal.set(format!("{next:03o}"));
        }
    }
}

fn apply_permissions(
    form: &PermissionsForm,
    parent: &gtk::ApplicationWindow,
    window: &gtk::Window,
    on_apply: &Once<u32>,
) {
    let Some(mode) = parse_octal_mode(&form.octal.get()) else {
        show_error(
            parent,
            &t!("error.properties_unavailable"),
            &t!("error.invalid_unix_mode"),
        );
        return;
    };
    on_apply.call(mode);
    window.close();
}

fn permissions_editor(form: &Rc<PermissionsForm>) {
    text(t!("dialog.properties_permissions"), |heading| {
        heading.set_margin_top(12);
        heading.css("heading");
    });

    hbox(8, |_| {
        text(t!("dialog.properties_mode_octal"), |_| {});
        entry(&form.octal, |entry| {
            entry.set_max_width_chars(5);
            entry.set_width_chars(5);
            entry.set_input_purpose(gtk::InputPurpose::Digits);

            let mode = form.mode.clone();
            entry.add_disposable(form.octal.observe_without_initial(move |text| {
                if let Some(parsed) = parse_octal_mode(text) {
                    mode.set(parsed);
                }
            }));
        });
        text(form.mode.map(|mode| format_symbolic_mode(*mode)), |label| {
            label.set_hexpand(true);
            label.css("dim-label");
            label.set_selectable(true);
        });
    });

    permission_grid(form);
}

fn permission_grid(form: &Rc<PermissionsForm>) -> Ctx<gtk::Grid> {
    mount(gtk::Grid::new(), |grid| {
        grid.set_column_spacing(16);
        grid.set_row_spacing(6);
        grid.set_margin_top(6);

        grid.attach(&gtk::Label::new(None), 0, 0, 1, 1);
        let columns = [
            t!("dialog.properties_perm_read"),
            t!("dialog.properties_perm_write"),
            t!("dialog.properties_perm_execute"),
        ];
        for (column, title) in (1..).zip(&columns) {
            let header = gtk::Label::new(Some(title));
            header.set_xalign(0.5);
            header.add_css_class("dim-label");
            grid.attach(&header, column, 0, 1, 1);
        }

        // Each class owns three adjacent bits, read first.
        let classes = [
            (t!("dialog.properties_perm_owner"), 0o400),
            (t!("dialog.properties_perm_group"), 0o040),
            (t!("dialog.properties_perm_other"), 0o004),
        ];
        for (row, (title, read_bit)) in (1..).zip(&classes) {
            let label = gtk::Label::new(Some(title));
            label.set_xalign(0.0);
            grid.attach(&label, 0, row, 1, 1);
            for column in 1..=3 {
                let check = permission_check(form, read_bit >> (column - 1));
                grid.attach(&check, column, row, 1, 1);
            }
        }
    })
}

/// A checkbox for one bit of the mode. A grid is not a DSL container, so the
/// box is attached by the caller and only adopted here for its bindings.
fn permission_check(form: &Rc<PermissionsForm>, mask: u32) -> gtk::CheckButton {
    let check = gtk::CheckButton::new();
    let bound = adopt(&check);
    bound.bind(&form.mode, move |check, mode| {
        check.set_active(mode & mask != 0);
    });
    let form = Rc::clone(form);
    let handler = check.connect_toggled(move |check| form.set_bit(mask, check.is_active()));
    bound.own_signal(handler);
    check
}

/// The rows every properties dialog starts with.
fn identity_rows(entry: &Entry, location: &PanelLocation) -> Vec<PropertyRow> {
    vec![
        (t!("dialog.properties_name"), entry.name.clone()),
        (
            t!("dialog.properties_location"),
            entry.display_path(location),
        ),
        (
            t!("dialog.properties_type"),
            presentation::entry_type_label(entry),
        ),
    ]
}

fn property_grid(rows: &[PropertyRow]) -> Ctx<gtk::Grid> {
    mount(gtk::Grid::new(), |grid| {
        grid.set_column_spacing(12);
        grid.set_row_spacing(8);
        grid.set_hexpand(true);

        for (row, (label, value)) in (0..).zip(rows) {
            let key = gtk::Label::new(Some(label));
            key.set_xalign(0.0);
            key.set_yalign(0.0);
            key.add_css_class("dim-label");

            let val = gtk::Label::new(Some(value));
            val.set_xalign(0.0);
            val.set_yalign(0.0);
            val.set_hexpand(true);
            val.set_wrap(true);
            val.set_wrap_mode(gtk::pango::WrapMode::WordChar);
            val.set_selectable(true);

            grid.attach(&key, 0, row, 1, 1);
            grid.attach(&val, 1, row, 1, 1);
        }
    })
}

fn property_values(
    entry: &Entry,
    location: &PanelLocation,
    calculated_dir_size: Option<u64>,
) -> (String, String, String, Option<String>) {
    if let Some(path) = location.entry_filesystem_path(entry) {
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            let size = if entry.is_dir {
                calculated_dir_size
                    .or_else(|| (entry.size_bytes > 0).then_some(entry.size_bytes))
                    .map(format_bytes)
                    .unwrap_or_else(|| t!("dialog.properties_size_folder").into_owned())
            } else {
                format_bytes(metadata.len())
            };
            let modified = metadata
                .modified()
                .ok()
                .map(format_system_time)
                .unwrap_or_else(|| "-".into());
            let created = metadata.created().ok().map(format_system_time);
            let attributes = if entry.attributes.is_empty() {
                "-".into()
            } else {
                entry.attributes.clone()
            };
            return (size, modified, attributes, created);
        }
    }

    let size = presentation::entry_size_label_with_calculated(entry, calculated_dir_size);
    let modified = presentation::entry_modified_label(entry);
    let attributes = {
        let label = presentation::entry_attributes_label(entry);
        if label.is_empty() {
            "-".into()
        } else {
            label
        }
    };
    (size, modified, attributes, None)
}

fn parse_octal_mode(text: &str) -> Option<u32> {
    u32::from_str_radix(text.trim(), 8)
        .ok()
        .map(|mode| mode & 0o777)
}

fn parse_remote_mode(attributes: &str, is_dir: bool) -> u32 {
    let trimmed = attributes.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("DIR") {
        return if is_dir { 0o755 } else { 0o644 };
    }
    match u32::from_str_radix(trimmed, 8) {
        Ok(mode) => mode & 0o777,
        Err(_) => {
            if is_dir {
                0o755
            } else {
                0o644
            }
        }
    }
}

fn format_symbolic_mode(mode: u32) -> String {
    let bit = |mask: u32, ch: char| if mode & mask != 0 { ch } else { '-' };
    format!(
        "{}{}{}{}{}{}{}{}{}",
        bit(0o400, 'r'),
        bit(0o200, 'w'),
        bit(0o100, 'x'),
        bit(0o040, 'r'),
        bit(0o020, 'w'),
        bit(0o010, 'x'),
        bit(0o004, 'r'),
        bit(0o002, 'w'),
        bit(0o001, 'x'),
    )
}
