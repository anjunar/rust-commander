use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
};

use gtk::{glib, prelude::*};
use rust_i18n::t;

use crate::{
    application::{ConflictResolution, FileOperationKind, OperationConflict, OperationPlan},
    fs::{operations::progress_percent, reader::format_bytes},
    presentation,
};

use super::dialogs_base::{build_modal_window, present_with_focus, show_error, ModalWindow};

pub fn confirm_operation<F>(parent: &gtk::ApplicationWindow, request: OperationPlan, on_confirm: F)
where
    F: FnOnce(OperationPlan) + 'static,
{
    let source_label = source_label(&request);
    let destination_text = editable_destination_text(&request);

    let (title, detail, confirm_label) = match request.kind() {
        FileOperationKind::Copy => (
            t!("dialog.copy_confirmation_title").into_owned(),
            t!(
                "dialog.copy_confirmation_detail",
                source = source_label.as_str()
            )
            .into_owned(),
            presentation::file_operation_label(&FileOperationKind::Copy),
        ),
        FileOperationKind::Move => (
            t!("dialog.move_confirmation_title").into_owned(),
            t!(
                "dialog.move_confirmation_detail",
                source = source_label.as_str()
            )
            .into_owned(),
            presentation::file_operation_label(&FileOperationKind::Move),
        ),
        FileOperationKind::Delete => {
            let permanent =
                matches!(&request, OperationPlan::Local(local) if !local.use_recycle_bin);
            if permanent {
                (
                    t!("dialog.delete_permanent_confirmation_title").into_owned(),
                    t!(
                        "dialog.delete_permanent_confirmation_detail",
                        source = source_label.as_str()
                    )
                    .into_owned(),
                    t!("common.delete_permanent").into_owned(),
                )
            } else {
                (
                    t!("dialog.delete_confirmation_title").into_owned(),
                    t!(
                        "dialog.delete_confirmation_detail",
                        source = source_label.as_str()
                    )
                    .into_owned(),
                    presentation::file_operation_label(&FileOperationKind::Delete),
                )
            }
        }
    };

    let has_destination_entry = destination_text.is_some();
    let ModalWindow {
        window,
        content,
        actions,
        ..
    } = build_modal_window(
        parent,
        &title,
        if has_destination_entry { 560 } else { 480 },
        if has_destination_entry { 180 } else { 160 },
    );

    let title_label = gtk::Label::new(Some(&title));
    title_label.set_xalign(0.0);
    title_label.set_wrap(true);
    title_label.add_css_class("dialog-title");
    content.append(&title_label);

    let detail_label = gtk::Label::new(Some(&detail));
    detail_label.set_xalign(0.0);
    detail_label.set_wrap(true);
    content.append(&detail_label);

    let destination_entry = destination_text.as_ref().map(|text| {
        let entry = gtk::Entry::new();
        entry.set_text(text);
        entry.set_hexpand(true);
        entry.set_activates_default(true);
        content.append(&entry);
        entry
    });

    let cancel_button = gtk::Button::with_label(&t!("common.cancel"));
    let confirm_button = gtk::Button::with_label(&confirm_label);
    confirm_button.add_css_class("suggested-action");
    actions.append(&cancel_button);
    actions.append(&confirm_button);
    window.set_default_widget(Some(&confirm_button));

    {
        let window = window.clone();
        cancel_button.connect_clicked(move |_| {
            window.close();
        });
    }

    let callback = Rc::new(RefCell::new(Some(on_confirm)));
    let request = Rc::new(RefCell::new(Some(request)));
    {
        let window = window.clone();
        let parent = parent.clone();
        let callback = Rc::clone(&callback);
        let request = Rc::clone(&request);
        let destination_entry = destination_entry.clone();
        confirm_button.connect_clicked(move |_| {
            let mut prepared = match request.borrow_mut().take() {
                Some(value) => value,
                None => return,
            };

            if let Some(entry) = &destination_entry {
                if let Err(detail) = apply_edited_destination(&mut prepared, &entry.text()) {
                    *request.borrow_mut() = Some(prepared);
                    show_error(&parent, &t!("error.operation_unavailable"), &detail);
                    return;
                }
            }

            window.close();
            if let Some(on_confirm) = callback.borrow_mut().take() {
                glib::idle_add_local_once(move || {
                    on_confirm(prepared);
                });
            }
        });
    }

    if let Some(entry) = destination_entry {
        let selection = destination_text
            .as_deref()
            .and_then(filename_selection_bounds);
        glib::idle_add_local_once(move || {
            window.present();
            entry.grab_focus();
            if let Some((start, end)) = selection {
                entry.select_region(start, end);
            } else {
                entry.select_region(0, -1);
            }
        });
    } else {
        present_with_focus(window, &confirm_button);
    }
}

pub fn show_conflict<F>(
    parent: &gtk::ApplicationWindow,
    conflict: OperationConflict,
    on_resolution: F,
) where
    F: FnOnce(ConflictResolution) + 'static,
{
    let detail = format!(
        "{}",
        t!(
            "dialog.conflict_detail",
            source = conflict.source.display().to_string(),
            target = conflict.target.display().to_string()
        )
    );
    let title = t!(
        "dialog.conflict_title",
        kind = presentation::file_operation_label(&conflict.kind)
    )
    .into_owned();

    let ModalWindow {
        window,
        content,
        actions,
        ..
    } = build_modal_window(parent, &title, 520, 240);

    let title_label = gtk::Label::new(Some(&title));
    title_label.set_xalign(0.0);
    title_label.set_wrap(true);
    title_label.add_css_class("dialog-title");
    content.append(&title_label);

    let detail_label = gtk::Label::new(Some(&detail));
    detail_label.set_xalign(0.0);
    detail_label.set_wrap(true);
    content.append(&detail_label);

    let cancel_button = gtk::Button::with_label(&t!("common.cancel"));
    let skip_button = gtk::Button::with_label(&t!("common.skip"));
    let rename_button = gtk::Button::with_label(&t!("common.rename"));
    let overwrite_button = gtk::Button::with_label(&t!("common.overwrite"));
    overwrite_button.add_css_class("suggested-action");
    actions.append(&cancel_button);
    actions.append(&skip_button);
    actions.append(&rename_button);
    actions.append(&overwrite_button);
    window.set_default_widget(Some(&overwrite_button));

    let callback = Rc::new(RefCell::new(Some(on_resolution)));
    let resolve = |resolution: ConflictResolution,
                   window: &gtk::Window,
                   callback: &Rc<RefCell<Option<F>>>| {
        if let Some(on_resolution) = callback.borrow_mut().take() {
            on_resolution(resolution);
        }
        window.close();
    };

    {
        let window = window.clone();
        let callback = Rc::clone(&callback);
        cancel_button.connect_clicked(move |_| {
            resolve(ConflictResolution::Cancel, &window, &callback);
        });
    }
    {
        let window = window.clone();
        let callback = Rc::clone(&callback);
        skip_button.connect_clicked(move |_| {
            resolve(ConflictResolution::Skip, &window, &callback);
        });
    }
    {
        let window = window.clone();
        let callback = Rc::clone(&callback);
        rename_button.connect_clicked(move |_| {
            resolve(ConflictResolution::Rename, &window, &callback);
        });
    }
    {
        let window = window.clone();
        let callback = Rc::clone(&callback);
        overwrite_button.connect_clicked(move |_| {
            resolve(ConflictResolution::Overwrite, &window, &callback);
        });
    }

    present_with_focus(window, &overwrite_button);
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
            ..
        } = build_modal_window(parent, title_text, 460, 160);

        let title = gtk::Label::new(Some(title_text));
        title.set_xalign(0.0);
        title.add_css_class("dialog-title");
        content.append(&title);

        let detail = gtk::Label::new(Some(&t!("progress.preparing_file_operation")));
        detail.set_xalign(0.0);
        detail.set_wrap(true);
        content.append(&detail);

        let progress = gtk::ProgressBar::new();
        progress.set_show_text(true);
        content.append(&progress);

        let eta = gtk::Label::new(Some(&t!("progress.eta_unknown")));
        eta.set_xalign(0.0);
        content.append(&eta);

        let cancel_button = gtk::Button::with_label(&t!("progress.cancel_operation"));
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

    pub fn update_progress(&self, snapshot: &crate::application::OperationSnapshot) {
        let fraction = progress_percent(snapshot);
        self.title.set_label(&t!(
            "progress.in_progress",
            kind = presentation::file_operation_label(&snapshot.kind)
        ));
        self.detail.set_label(&t!(
            "progress.file_operation_detail",
            processed = format_bytes(snapshot.processed_bytes),
            total = format_bytes(snapshot.total_bytes),
            processed_entries = snapshot.processed_entries,
            total_entries = snapshot.total_entries,
            item = snapshot.current_item.as_str()
        ));
        self.progress.set_fraction(fraction);
        self.progress
            .set_text(Some(&format!("{:.0}%", fraction * 100.0)));
        self.eta
            .set_label(&crate::fs::operations::format_eta(snapshot));
    }

    pub fn update_archive_progress(&self, progress: &crate::archive::ArchiveProgress) {
        self.title
            .set_label(&t!("progress.archive_copy_in_progress"));

        let detail = progress.current_path.clone().unwrap_or_else(|| {
            progress
                .operation
                .as_ref()
                .map(|operation| match operation {
                    crate::archive::ArchiveOperation::ExtractEntries { entry_paths } => {
                        t!("progress.selected_archive_items", count = entry_paths.len())
                            .into_owned()
                    }
                })
                .unwrap_or_else(|| t!("progress.preparing_archive_operation").into_owned())
        });
        self.detail.set_label(&detail);

        if let Some(percent) = progress.percent {
            let clamped = percent.clamp(0.0, 1.0);
            self.progress.set_fraction(clamped);
            self.progress
                .set_text(Some(&format!("{:.0}%", clamped * 100.0)));
        } else {
            self.progress.pulse();
            self.progress.set_text(Some(&t!("progress.working")));
        }

        let processed_entries = progress.processed_entries.unwrap_or(0);
        let total_entries = progress.total_entries.unwrap_or(0);
        self.eta.set_label(&t!(
            "progress.items_count",
            processed = processed_entries,
            total = total_entries
        ));
    }

    pub fn set_waiting_for_conflict(&self) {
        self.detail
            .set_label(&t!("progress.waiting_for_conflict_resolution"));
        self.progress.pulse();
    }

    pub fn close(&self) {
        self.window.close();
    }
}

fn source_label(request: &OperationPlan) -> String {
    match request {
        OperationPlan::ArchiveExtract(request) => {
            if request.entry_paths.len() == 1 {
                request.entry_paths[0].clone()
            } else {
                t!(
                    "progress.selected_archive_items",
                    count = request.entry_paths.len()
                )
                .into_owned()
            }
        }
        OperationPlan::RemoteDownload(request) => {
            if request.entry_paths.len() == 1 {
                request.entry_paths[0].clone()
            } else {
                t!(
                    "progress.selected_archive_items",
                    count = request.entry_paths.len()
                )
                .into_owned()
            }
        }
        OperationPlan::Local(request) => selected_paths_summary(&request.sources),
        OperationPlan::RemoteUpload(request) => selected_paths_summary(&request.sources),
    }
}

fn editable_destination_text(request: &OperationPlan) -> Option<String> {
    match request {
        OperationPlan::Local(request)
            if matches!(
                request.kind,
                FileOperationKind::Copy | FileOperationKind::Move
            ) =>
        {
            let directory = request.target_directory.as_ref()?;
            if request.sources.len() == 1 {
                let name = request.sources[0].file_name()?;
                Some(directory.join(name).display().to_string())
            } else {
                Some(directory.display().to_string())
            }
        }
        OperationPlan::ArchiveExtract(request) => {
            Some(request.target_directory.display().to_string())
        }
        OperationPlan::RemoteDownload(request) => {
            Some(request.target_directory.display().to_string())
        }
        OperationPlan::RemoteUpload(request) => Some(request.target_directory.clone()),
        OperationPlan::Local(_) => None,
    }
}

fn apply_edited_destination(request: &mut OperationPlan, edited: &str) -> Result<(), String> {
    let trimmed = edited.trim();
    if trimmed.is_empty() {
        return Err(t!("error.empty_destination").into_owned());
    }

    match request {
        OperationPlan::Local(local) => apply_local_destination(local, trimmed),
        OperationPlan::ArchiveExtract(request) => {
            request.target_directory = PathBuf::from(trimmed);
            Ok(())
        }
        OperationPlan::RemoteDownload(request) => {
            request.target_directory = PathBuf::from(trimmed);
            Ok(())
        }
        OperationPlan::RemoteUpload(request) => {
            request.target_directory = trimmed.to_string();
            Ok(())
        }
    }
}

fn apply_local_destination(
    local: &mut crate::application::LocalOperationRequest,
    trimmed: &str,
) -> Result<(), String> {
    let path = PathBuf::from(trimmed);
    let has_separator = trimmed.contains('/') || trimmed.contains('\\');

    if local.sources.len() == 1 {
        let treat_as_directory = trimmed.ends_with('/') || trimmed.ends_with('\\') || path.is_dir();
        if treat_as_directory {
            local.target_directory = Some(path);
            local.target_path = None;
        } else if !has_separator {
            let directory = local
                .target_directory
                .clone()
                .ok_or_else(|| t!("error.invalid_destination").into_owned())?;
            local.target_path = Some(directory.join(trimmed));
        } else {
            let parent = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .ok_or_else(|| t!("error.invalid_destination").into_owned())?;
            local.target_directory = Some(parent.to_path_buf());
            local.target_path = Some(path);
        }
    } else {
        local.target_directory = Some(path);
        local.target_path = None;
    }

    Ok(())
}

fn filename_selection_bounds(path: &str) -> Option<(i32, i32)> {
    let name = Path::new(path).file_name()?.to_str()?;
    if name.is_empty() || !path.ends_with(name) {
        return None;
    }

    let total = path.chars().count() as i32;
    let name_len = name.chars().count() as i32;
    Some((total - name_len, total))
}

pub(crate) fn selected_paths_summary(paths: &[PathBuf]) -> String {
    if paths.len() == 1 {
        return paths[0].display().to_string();
    }

    t!("common.items_count", count = paths.len()).into_owned()
}
