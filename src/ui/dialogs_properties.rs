use std::{
    cell::{Cell, RefCell},
    fs,
    rc::Rc,
};

use gtk::prelude::*;
use rust_i18n::t;

use crate::{
    domain::{Entry, PanelLocation},
    fs::reader::{format_bytes, format_system_time},
    presentation,
};

use super::dialogs_base::{build_modal_window, present_with_focus, show_error, ModalWindow};

/// Show a read-only properties dialog for the selected panel entry.
pub fn show_properties(
    parent: &gtk::ApplicationWindow,
    entry: &Entry,
    location: &PanelLocation,
    calculated_dir_size: Option<u64>,
) {
    let ModalWindow {
        window,
        content,
        actions,
    } = build_modal_window(parent, &t!("dialog.properties_title"), 480, 320);

    let grid = gtk::Grid::new();
    grid.set_column_spacing(12);
    grid.set_row_spacing(8);
    grid.set_hexpand(true);

    let mut row = 0;
    append_property_row(
        &grid,
        &mut row,
        &t!("dialog.properties_name"),
        &entry.name,
    );
    append_property_row(
        &grid,
        &mut row,
        &t!("dialog.properties_location"),
        &entry.display_path(location),
    );
    append_property_row(
        &grid,
        &mut row,
        &t!("dialog.properties_type"),
        &presentation::entry_type_label(entry),
    );

    let (size_label, modified_label, attributes_label, created_label) =
        property_values(entry, location, calculated_dir_size);

    append_property_row(
        &grid,
        &mut row,
        &t!("dialog.properties_size"),
        &size_label,
    );
    if let Some(created) = created_label {
        append_property_row(
            &grid,
            &mut row,
            &t!("dialog.properties_created"),
            &created,
        );
    }
    append_property_row(
        &grid,
        &mut row,
        &t!("dialog.properties_modified"),
        &modified_label,
    );
    append_property_row(
        &grid,
        &mut row,
        &t!("dialog.properties_attributes"),
        &attributes_label,
    );

    content.append(&grid);

    let close = gtk::Button::with_label(&t!("common.close"));
    close.add_css_class("suggested-action");
    {
        let window = window.clone();
        close.connect_clicked(move |_| {
            window.close();
        });
    }
    actions.append(&close);

    present_with_focus(window, &close);
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
    let ModalWindow {
        window,
        content,
        actions,
    } = build_modal_window(parent, &t!("dialog.properties_title"), 520, 420);

    let grid = gtk::Grid::new();
    grid.set_column_spacing(12);
    grid.set_row_spacing(8);
    grid.set_hexpand(true);

    let mut row = 0;
    append_property_row(
        &grid,
        &mut row,
        &t!("dialog.properties_name"),
        &entry.name,
    );
    append_property_row(
        &grid,
        &mut row,
        &t!("dialog.properties_location"),
        &entry.display_path(location),
    );
    append_property_row(
        &grid,
        &mut row,
        &t!("dialog.properties_type"),
        &presentation::entry_type_label(entry),
    );

    let (size_label, modified_label, _, _) =
        property_values(entry, location, calculated_dir_size);
    append_property_row(
        &grid,
        &mut row,
        &t!("dialog.properties_size"),
        &size_label,
    );
    append_property_row(
        &grid,
        &mut row,
        &t!("dialog.properties_modified"),
        &modified_label,
    );
    content.append(&grid);

    let initial_mode = parse_remote_mode(&entry.attributes, entry.is_dir);

    let permissions_heading = gtk::Label::new(Some(&t!("dialog.properties_permissions")));
    permissions_heading.set_xalign(0.0);
    permissions_heading.set_margin_top(12);
    permissions_heading.add_css_class("heading");
    content.append(&permissions_heading);

    let mode_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let mode_label = gtk::Label::new(Some(&t!("dialog.properties_mode_octal")));
    mode_label.set_xalign(0.0);
    let mode_entry = gtk::Entry::new();
    mode_entry.set_max_width_chars(5);
    mode_entry.set_width_chars(5);
    mode_entry.set_text(&format!("{:03o}", initial_mode & 0o777));
    mode_entry.set_input_purpose(gtk::InputPurpose::Digits);
    let symbolic_label = gtk::Label::new(Some(&format_symbolic_mode(initial_mode)));
    symbolic_label.set_xalign(0.0);
    symbolic_label.set_hexpand(true);
    symbolic_label.add_css_class("dim-label");
    symbolic_label.set_selectable(true);
    mode_row.append(&mode_label);
    mode_row.append(&mode_entry);
    mode_row.append(&symbolic_label);
    content.append(&mode_row);

    let checks = PermissionChecks::new(initial_mode);
    content.append(&checks.build_grid());

    let syncing = Rc::new(Cell::new(false));
    {
        let checks = checks.clone();
        let mode_entry = mode_entry.clone();
        let symbolic_label = symbolic_label.clone();
        let syncing = Rc::clone(&syncing);
        mode_entry.connect_changed(move |entry| {
            if syncing.get() {
                return;
            }
            let text = entry.text();
            let Ok(mode) = u32::from_str_radix(text.trim(), 8) else {
                return;
            };
            let mode = mode & 0o777;
            syncing.set(true);
            checks.apply_mode(mode);
            symbolic_label.set_label(&format_symbolic_mode(mode));
            syncing.set(false);
        });
    }
    {
        let checks_cb = checks.clone();
        let mode_entry = mode_entry.clone();
        let symbolic_label = symbolic_label.clone();
        let syncing = Rc::clone(&syncing);
        checks.connect_toggled(move || {
            if syncing.get() {
                return;
            }
            let mode = checks_cb.current_mode();
            syncing.set(true);
            mode_entry.set_text(&format!("{:03o}", mode));
            symbolic_label.set_label(&format_symbolic_mode(mode));
            syncing.set(false);
        });
    }

    let cancel = gtk::Button::with_label(&t!("common.cancel"));
    let apply = gtk::Button::with_label(&t!("common.apply"));
    apply.add_css_class("suggested-action");
    actions.append(&cancel);
    actions.append(&apply);
    window.set_default_widget(Some(&apply));

    {
        let window = window.clone();
        cancel.connect_clicked(move |_| {
            window.close();
        });
    }

    let on_apply = Rc::new(RefCell::new(Some(on_apply_permissions)));
    {
        let window = window.clone();
        let mode_entry = mode_entry.clone();
        let parent = parent.clone();
        apply.connect_clicked(move |_| {
            let mode = match u32::from_str_radix(mode_entry.text().trim(), 8) {
                Ok(mode) => mode & 0o777,
                Err(_) => {
                    show_error(
                        &parent,
                        &t!("error.properties_unavailable"),
                        &t!("error.invalid_unix_mode"),
                    );
                    return;
                }
            };
            if let Some(callback) = on_apply.borrow_mut().take() {
                callback(mode);
            }
            window.close();
        });
    }

    present_with_focus(window, &apply);
}

#[derive(Clone)]
struct PermissionChecks {
    owner_r: gtk::CheckButton,
    owner_w: gtk::CheckButton,
    owner_x: gtk::CheckButton,
    group_r: gtk::CheckButton,
    group_w: gtk::CheckButton,
    group_x: gtk::CheckButton,
    other_r: gtk::CheckButton,
    other_w: gtk::CheckButton,
    other_x: gtk::CheckButton,
}

impl PermissionChecks {
    fn new(mode: u32) -> Self {
        let checks = Self {
            owner_r: gtk::CheckButton::new(),
            owner_w: gtk::CheckButton::new(),
            owner_x: gtk::CheckButton::new(),
            group_r: gtk::CheckButton::new(),
            group_w: gtk::CheckButton::new(),
            group_x: gtk::CheckButton::new(),
            other_r: gtk::CheckButton::new(),
            other_w: gtk::CheckButton::new(),
            other_x: gtk::CheckButton::new(),
        };
        checks.apply_mode(mode);
        checks
    }

    fn build_grid(&self) -> gtk::Grid {
        let grid = gtk::Grid::new();
        grid.set_column_spacing(16);
        grid.set_row_spacing(6);
        grid.set_margin_top(6);

        let header = |text: &str| {
            let label = gtk::Label::new(Some(text));
            label.set_xalign(0.5);
            label.add_css_class("dim-label");
            label
        };
        grid.attach(&gtk::Label::new(None), 0, 0, 1, 1);
        grid.attach(&header(&t!("dialog.properties_perm_read")), 1, 0, 1, 1);
        grid.attach(&header(&t!("dialog.properties_perm_write")), 2, 0, 1, 1);
        grid.attach(&header(&t!("dialog.properties_perm_execute")), 3, 0, 1, 1);

        let row_label = |text: &str| {
            let label = gtk::Label::new(Some(text));
            label.set_xalign(0.0);
            label
        };
        grid.attach(&row_label(&t!("dialog.properties_perm_owner")), 0, 1, 1, 1);
        grid.attach(&self.owner_r, 1, 1, 1, 1);
        grid.attach(&self.owner_w, 2, 1, 1, 1);
        grid.attach(&self.owner_x, 3, 1, 1, 1);

        grid.attach(&row_label(&t!("dialog.properties_perm_group")), 0, 2, 1, 1);
        grid.attach(&self.group_r, 1, 2, 1, 1);
        grid.attach(&self.group_w, 2, 2, 1, 1);
        grid.attach(&self.group_x, 3, 2, 1, 1);

        grid.attach(&row_label(&t!("dialog.properties_perm_other")), 0, 3, 1, 1);
        grid.attach(&self.other_r, 1, 3, 1, 1);
        grid.attach(&self.other_w, 2, 3, 1, 1);
        grid.attach(&self.other_x, 3, 3, 1, 1);

        grid
    }

    fn apply_mode(&self, mode: u32) {
        let mode = mode & 0o777;
        self.owner_r.set_active(mode & 0o400 != 0);
        self.owner_w.set_active(mode & 0o200 != 0);
        self.owner_x.set_active(mode & 0o100 != 0);
        self.group_r.set_active(mode & 0o040 != 0);
        self.group_w.set_active(mode & 0o020 != 0);
        self.group_x.set_active(mode & 0o010 != 0);
        self.other_r.set_active(mode & 0o004 != 0);
        self.other_w.set_active(mode & 0o002 != 0);
        self.other_x.set_active(mode & 0o001 != 0);
    }

    fn current_mode(&self) -> u32 {
        let mut mode = 0u32;
        if self.owner_r.is_active() {
            mode |= 0o400;
        }
        if self.owner_w.is_active() {
            mode |= 0o200;
        }
        if self.owner_x.is_active() {
            mode |= 0o100;
        }
        if self.group_r.is_active() {
            mode |= 0o040;
        }
        if self.group_w.is_active() {
            mode |= 0o020;
        }
        if self.group_x.is_active() {
            mode |= 0o010;
        }
        if self.other_r.is_active() {
            mode |= 0o004;
        }
        if self.other_w.is_active() {
            mode |= 0o002;
        }
        if self.other_x.is_active() {
            mode |= 0o001;
        }
        mode
    }

    fn connect_toggled<F>(&self, f: F)
    where
        F: Fn() + 'static,
    {
        let f = Rc::new(f);
        for button in [
            &self.owner_r,
            &self.owner_w,
            &self.owner_x,
            &self.group_r,
            &self.group_w,
            &self.group_x,
            &self.other_r,
            &self.other_w,
            &self.other_x,
        ] {
            let f = Rc::clone(&f);
            button.connect_toggled(move |_| f());
        }
    }
}

fn append_property_row(grid: &gtk::Grid, row: &mut i32, label: &str, value: &str) {
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

    grid.attach(&key, 0, *row, 1, 1);
    grid.attach(&val, 1, *row, 1, 1);
    *row += 1;
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
