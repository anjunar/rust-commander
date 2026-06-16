#![allow(deprecated)]

use gtk::{glib, prelude::*};

use crate::{
    domain::operation::{
        ConflictResolution, FileOperationKind, FileOperationRequest, OperationConflict,
    },
    fs::{operations::progress_percent, reader::format_bytes},
};

pub fn show_error(parent: &gtk::ApplicationWindow, title: &str, detail: &str) {
    let dialog = gtk::MessageDialog::builder()
        .transient_for(parent)
        .modal(true)
        .message_type(gtk::MessageType::Error)
        .buttons(gtk::ButtonsType::Close)
        .text(title)
        .secondary_text(detail)
        .build();

    glib::MainContext::default().spawn_local(async move {
        dialog.run_future().await;
        dialog.close();
    });
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

    let dialog = gtk::MessageDialog::builder()
        .transient_for(parent)
        .modal(true)
        .message_type(gtk::MessageType::Question)
        .buttons(gtk::ButtonsType::None)
        .text(title)
        .secondary_text(&detail)
        .build();
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button(confirm_label, gtk::ResponseType::Accept);
    dialog.set_default_response(gtk::ResponseType::Accept);

    glib::MainContext::default().spawn_local(async move {
        let response = dialog.run_future().await;
        dialog.close();
        if response == gtk::ResponseType::Accept {
            on_confirm(request);
        }
    });
}

pub fn prompt_rename<F>(parent: &gtk::ApplicationWindow, current_name: String, on_confirm: F)
where
    F: FnOnce(String) + 'static,
{
    let dialog = gtk::Dialog::with_buttons(
        Some("Rename"),
        Some(parent),
        gtk::DialogFlags::MODAL,
        &[
            ("Cancel", gtk::ResponseType::Cancel),
            ("Rename", gtk::ResponseType::Accept),
        ],
    );
    dialog.set_default_response(gtk::ResponseType::Accept);

    let content = dialog.content_area();
    content.set_spacing(12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);

    let label = gtk::Label::new(Some("Enter a new name:"));
    label.set_xalign(0.0);
    content.append(&label);

    let entry = gtk::Entry::new();
    entry.set_text(&current_name);
    entry.set_activates_default(true);
    entry.set_hexpand(true);
    content.append(&entry);

    glib::MainContext::default().spawn_local(async move {
        dialog.present();
        entry.grab_focus();
        entry.select_region(0, -1);
        let response = dialog.run_future().await;
        let value = entry.text().to_string();
        dialog.close();
        if response == gtk::ResponseType::Accept {
            on_confirm(value);
        }
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
    let dialog = gtk::MessageDialog::builder()
        .transient_for(parent)
        .modal(true)
        .message_type(gtk::MessageType::Warning)
        .buttons(gtk::ButtonsType::None)
        .text(format!("{} conflict", conflict.kind.label()))
        .secondary_text(&detail)
        .build();
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button("Skip", gtk::ResponseType::No);
    dialog.add_button("Rename", gtk::ResponseType::Apply);
    dialog.add_button("Overwrite", gtk::ResponseType::Accept);
    dialog.set_default_response(gtk::ResponseType::No);

    glib::MainContext::default().spawn_local(async move {
        let response = dialog.run_future().await;
        dialog.close();
        let resolution = match response {
            gtk::ResponseType::Accept => ConflictResolution::Overwrite,
            gtk::ResponseType::No => ConflictResolution::Skip,
            gtk::ResponseType::Apply => ConflictResolution::Rename,
            _ => ConflictResolution::Cancel,
        };
        on_resolution(resolution);
    });
}

#[derive(Clone)]
pub struct ProgressDialog {
    dialog: gtk::Dialog,
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
        let dialog = gtk::Dialog::with_buttons(
            Some(title_text),
            Some(parent),
            gtk::DialogFlags::MODAL,
            &[("Cancel operation", gtk::ResponseType::Cancel)],
        );
        dialog.set_default_size(460, 160);

        let content = dialog.content_area();
        content.set_spacing(10);
        content.set_margin_top(14);
        content.set_margin_bottom(14);
        content.set_margin_start(14);
        content.set_margin_end(14);

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

        dialog.connect_response(move |dialog, response| {
            if response == gtk::ResponseType::Cancel {
                on_cancel();
                dialog.set_sensitive(false);
            }
        });

        dialog.present();

        Self {
            dialog,
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
        self.dialog.close();
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
