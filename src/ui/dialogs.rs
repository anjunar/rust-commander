use std::{cell::RefCell, path::PathBuf, rc::Rc};

use gtk::{glib, prelude::*};

use crate::{
    config::AppConfig,
    domain::operation::{
        ConflictResolution, FileOperationKind, FileOperationRequest, OperationConflict,
    },
    fs::{operations::progress_percent, reader::format_bytes},
};

pub(crate) struct ModalWindow {
    pub window: gtk::Window,
    pub content: gtk::Box,
    pub actions: gtk::Box,
}

pub(crate) fn build_modal_window(
    parent: &gtk::ApplicationWindow,
    title: &str,
    default_width: i32,
    default_height: i32,
) -> ModalWindow {
    let window = gtk::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title(title)
        .default_width(default_width)
        .default_height(default_height)
        .build();

    let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    actions.set_halign(gtk::Align::End);

    root.append(&content);
    root.append(&actions);
    window.set_child(Some(&root));

    ModalWindow {
        window,
        content,
        actions,
    }
}

pub fn show_error(parent: &gtk::ApplicationWindow, title: &str, detail: &str) {
    gtk::AlertDialog::builder()
        .modal(true)
        .message(title)
        .detail(detail)
        .buttons(["Close"])
        .cancel_button(0)
        .default_button(0)
        .build()
        .show(Some(parent));
}

pub fn confirm_operation<F>(
    parent: &gtk::ApplicationWindow,
    request: FileOperationRequest,
    on_confirm: F,
) where
    F: FnOnce(FileOperationRequest) + 'static,
{
    let source_label = source_label(&request);
    let target_label = request
        .target_directory
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "-".into());

    let (title, detail, confirm_label) = match request.kind {
        FileOperationKind::Copy => (
            "Copy confirmation",
            format!("Copy {source_label} to {target_label}?"),
            "Copy",
        ),
        FileOperationKind::Move => (
            "Move confirmation",
            format!("Move {source_label} to {target_label}?"),
            "Move",
        ),
        FileOperationKind::Delete => (
            "Delete confirmation",
            format!("Delete {source_label}?"),
            "Delete",
        ),
    };

    let dialog = gtk::AlertDialog::builder()
        .modal(true)
        .message(title)
        .detail(&detail)
        .buttons(["Cancel", confirm_label])
        .cancel_button(0)
        .default_button(1)
        .build();

    dialog.choose(
        Some(parent),
        None::<&gtk::gio::Cancellable>,
        move |response| {
            if matches!(response, Ok(1)) {
                on_confirm(request);
            }
        },
    );
}

pub fn prompt_rename<F>(parent: &gtk::ApplicationWindow, current_name: String, on_confirm: F)
where
    F: FnOnce(String) + 'static,
{
    let ModalWindow {
        window,
        content,
        actions,
    } = build_modal_window(parent, "Rename", 420, 120);

    let label = gtk::Label::new(Some("Enter a new name:"));
    label.set_xalign(0.0);
    content.append(&label);

    let entry = gtk::Entry::new();
    entry.set_text(&current_name);
    entry.set_hexpand(true);
    content.append(&entry);

    let cancel_button = gtk::Button::with_label("Cancel");
    let confirm_button = gtk::Button::with_label("Rename");
    confirm_button.add_css_class("suggested-action");
    actions.append(&cancel_button);
    actions.append(&confirm_button);
    window.set_default_widget(Some(&confirm_button));

    let callback = std::rc::Rc::new(std::cell::RefCell::new(Some(on_confirm)));
    {
        let window = window.clone();
        cancel_button.connect_clicked(move |_| {
            window.close();
        });
    }
    {
        let window = window.clone();
        let entry = entry.clone();
        let callback = std::rc::Rc::clone(&callback);
        confirm_button.connect_clicked(move |_| {
            let value = entry.text().to_string();
            if let Some(on_confirm) = callback.borrow_mut().take() {
                on_confirm(value);
            }
            window.close();
        });
    }

    glib::idle_add_local_once(move || {
        window.present();
        entry.grab_focus();
        entry.select_region(0, -1);
    });
}

pub fn prompt_new_directory<F>(parent: &gtk::ApplicationWindow, on_confirm: F)
where
    F: FnOnce(String) + 'static,
{
    let ModalWindow {
        window,
        content,
        actions,
    } = build_modal_window(parent, "Create directory", 420, 120);

    let label = gtk::Label::new(Some("Enter a name for the new directory:"));
    label.set_xalign(0.0);
    content.append(&label);

    let entry = gtk::Entry::new();
    entry.set_hexpand(true);
    content.append(&entry);

    let cancel_button = gtk::Button::with_label("Cancel");
    let confirm_button = gtk::Button::with_label("Create");
    confirm_button.add_css_class("suggested-action");
    actions.append(&cancel_button);
    actions.append(&confirm_button);
    window.set_default_widget(Some(&confirm_button));

    let callback = std::rc::Rc::new(std::cell::RefCell::new(Some(on_confirm)));
    {
        let window = window.clone();
        cancel_button.connect_clicked(move |_| {
            window.close();
        });
    }
    {
        let window = window.clone();
        let entry = entry.clone();
        let callback = std::rc::Rc::clone(&callback);
        confirm_button.connect_clicked(move |_| {
            let value = entry.text().to_string();
            if let Some(on_confirm) = callback.borrow_mut().take() {
                on_confirm(value);
            }
            window.close();
        });
    }

    glib::idle_add_local_once(move || {
        window.present();
        entry.grab_focus();
    });
}

pub fn show_settings<F>(parent: &gtk::ApplicationWindow, current_config: AppConfig, on_save: F)
where
    F: FnOnce(AppConfig) + 'static,
{
    let ModalWindow {
        window,
        content,
        actions,
    } = build_modal_window(parent, "Application settings", 620, 220);

    let title = gtk::Label::new(Some("Application settings"));
    title.set_xalign(0.0);
    title.add_css_class("dialog-title");
    content.append(&title);

    let description = gtk::Label::new(Some(
        "Configure external tools used by RCommander.",
    ));
    description.set_xalign(0.0);
    description.set_wrap(true);
    content.append(&description);

    let field_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    content.append(&field_box);

    let field_label = gtk::Label::new(Some("7-Zip executable"));
    field_label.set_xalign(0.0);
    field_box.append(&field_label);

    let input_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    field_box.append(&input_row);

    let seven_zip_entry = gtk::Entry::new();
    seven_zip_entry.set_hexpand(true);
    seven_zip_entry.set_placeholder_text(Some(
        "Leave empty to use the bundled/default 7-Zip location",
    ));
    if let Some(path) = &current_config.archive.seven_zip_path {
        seven_zip_entry.set_text(&path.display().to_string());
    }
    input_row.append(&seven_zip_entry);

    let browse_button = gtk::Button::with_label("Browse...");
    input_row.append(&browse_button);

    let hint = gtk::Label::new(Some(
        "Example on Windows: C:\\Program Files\\7-Zip\\7z.exe",
    ));
    hint.set_xalign(0.0);
    hint.add_css_class("dim-label");
    field_box.append(&hint);

    let cancel_button = gtk::Button::with_label("Cancel");
    let save_button = gtk::Button::with_label("Save");
    save_button.add_css_class("suggested-action");
    actions.append(&cancel_button);
    actions.append(&save_button);
    window.set_default_widget(Some(&save_button));

    {
        let window = window.clone();
        cancel_button.connect_clicked(move |_| {
            window.close();
        });
    }

    {
        let parent = parent.clone();
        let entry = seven_zip_entry.clone();
        browse_button.connect_clicked(move |_| {
            let chooser = gtk::FileDialog::builder()
                .title("Select 7-Zip executable")
                .modal(true)
                .build();
            let entry = entry.clone();
            chooser.open(
                Some(&parent),
                None::<&gtk::gio::Cancellable>,
                move |result| {
                    if let Ok(file) = result
                        && let Some(path) = file.path()
                    {
                    entry.set_text(&path.display().to_string());
                }
                },
            );
        });
    }

    let callback = Rc::new(RefCell::new(Some(on_save)));
    {
        let window = window.clone();
        let entry = seven_zip_entry.clone();
        let callback = Rc::clone(&callback);
        let current_config = current_config.clone();
        save_button.connect_clicked(move |_| {
            let mut next_config = current_config.clone();
            let value = entry.text().trim().to_string();
            next_config.archive.seven_zip_path = if value.is_empty() {
                None
            } else {
                Some(PathBuf::from(value))
            };

            if let Some(on_save) = callback.borrow_mut().take() {
                on_save(next_config);
            }
            window.close();
        });
    }

    glib::idle_add_local_once(move || {
        window.present();
        seven_zip_entry.grab_focus();
    });
}

pub fn show_conflict<F>(
    parent: &gtk::ApplicationWindow,
    conflict: OperationConflict,
    on_resolution: F,
) where
    F: FnOnce(ConflictResolution) + 'static,
{
    let detail = format!(
        "The target already exists.\n\nSource: {}\nTarget: {}",
        conflict.source.display(),
        conflict.target.display()
    );
    let dialog = gtk::AlertDialog::builder()
        .modal(true)
        .message(format!("{} conflict", conflict.kind.label()))
        .detail(&detail)
        .buttons(["Cancel", "Skip", "Rename", "Overwrite"])
        .cancel_button(0)
        .default_button(1)
        .build();

    dialog.choose(
        Some(parent),
        None::<&gtk::gio::Cancellable>,
        move |response| {
            let resolution = match response {
                Ok(3) => ConflictResolution::Overwrite,
                Ok(1) => ConflictResolution::Skip,
                Ok(2) => ConflictResolution::Rename,
                _ => ConflictResolution::Cancel,
            };
            on_resolution(resolution);
        },
    );
}

#[derive(Clone)]
pub struct ProgressDialog {
    window: gtk::Window,
    title: gtk::Label,
    detail: gtk::Label,
    eta: gtk::Label,
    progress: gtk::ProgressBar,
}

impl ProgressDialog {
    pub fn new<F>(parent: &gtk::ApplicationWindow, title_text: &str, on_cancel: F) -> Self
    where
        F: Fn() + 'static,
    {
        let ModalWindow {
            window,
            content,
            actions,
        } = build_modal_window(parent, title_text, 460, 160);

        let title = gtk::Label::new(Some(title_text));
        title.set_xalign(0.0);
        title.add_css_class("dialog-title");
        content.append(&title);

        let detail = gtk::Label::new(Some("Preparing file operation..."));
        detail.set_xalign(0.0);
        detail.set_wrap(true);
        content.append(&detail);

        let progress = gtk::ProgressBar::new();
        progress.set_show_text(true);
        content.append(&progress);

        let eta = gtk::Label::new(Some("ETA --:--"));
        eta.set_xalign(0.0);
        content.append(&eta);

        let cancel_button = gtk::Button::with_label("Cancel operation");
        actions.append(&cancel_button);

        let window_for_cancel = window.clone();
        cancel_button.connect_clicked(move |_| {
            on_cancel();
            window_for_cancel.set_sensitive(false);
        });

        window.present();

        Self {
            window,
            title,
            detail,
            eta,
            progress,
        }
    }

    pub fn update_progress(&self, snapshot: &crate::domain::operation::OperationSnapshot) {
        let fraction = progress_percent(snapshot);
        self.title
            .set_label(&format!("{} in progress", snapshot.kind.label()));
        self.detail.set_label(&format!(
            "{} of {} | {} / {} items\n{}",
            format_bytes(snapshot.processed_bytes),
            format_bytes(snapshot.total_bytes),
            snapshot.processed_entries,
            snapshot.total_entries,
            snapshot.current_item
        ));
        self.progress.set_fraction(fraction);
        self.progress
            .set_text(Some(&format!("{:.0}%", fraction * 100.0)));
        self.eta
            .set_label(&crate::fs::operations::format_eta(snapshot));
    }

    pub fn set_waiting_for_conflict(&self) {
        self.detail.set_label("Waiting for conflict resolution...");
        self.progress.pulse();
    }

    pub fn close(&self) {
        self.window.close();
    }
}

fn source_label(request: &FileOperationRequest) -> String {
    if request.sources.len() == 1 {
        request
            .sources
            .first()
            .and_then(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| request.sources[0].display().to_string())
    } else {
        format!("{} items", request.sources.len())
    }
}
