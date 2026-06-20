use std::{cell::RefCell, path::PathBuf, rc::Rc};

use gtk::{glib, prelude::*};
use rust_i18n::t;

use crate::{
    application::{ConflictResolution, FileOperationKind, OperationConflict, OperationPlan},
    fs::{operations::progress_percent, reader::format_bytes},
    presentation,
};

use super::dialogs_base::{build_modal_window, ModalWindow};

pub fn confirm_operation<F>(parent: &gtk::ApplicationWindow, request: OperationPlan, on_confirm: F)
where
    F: FnOnce(OperationPlan) + 'static,
{
    let source_label = source_label(&request);
    let target_label = target_label(&request);

    let (title, detail, confirm_label) = match request.kind() {
        FileOperationKind::Copy => (
            t!("dialog.copy_confirmation_title").into_owned(),
            t!(
                "dialog.copy_confirmation_detail",
                source = source_label.as_str(),
                target = target_label.as_str()
            )
            .into_owned(),
            presentation::file_operation_label(&FileOperationKind::Copy),
        ),
        FileOperationKind::Move => (
            t!("dialog.move_confirmation_title").into_owned(),
            t!(
                "dialog.move_confirmation_detail",
                source = source_label.as_str(),
                target = target_label.as_str()
            )
            .into_owned(),
            presentation::file_operation_label(&FileOperationKind::Move),
        ),
        FileOperationKind::Delete => (
            t!("dialog.delete_confirmation_title").into_owned(),
            t!(
                "dialog.delete_confirmation_detail",
                source = source_label.as_str()
            )
            .into_owned(),
            presentation::file_operation_label(&FileOperationKind::Delete),
        ),
    };

    let ModalWindow {
        window,
        content,
        actions,
    } = build_modal_window(parent, &title, 480, 160);

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
        let callback = Rc::clone(&callback);
        let request = Rc::clone(&request);
        confirm_button.connect_clicked(move |_| {
            window.close();
            if let (Some(on_confirm), Some(request)) =
                (callback.borrow_mut().take(), request.borrow_mut().take())
            {
                glib::idle_add_local_once(move || {
                    on_confirm(request);
                });
            }
        });
    }

    window.present();
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
    window.set_default_widget(Some(&skip_button));

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

    window.present();
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

fn target_label(request: &OperationPlan) -> String {
    match request {
        OperationPlan::ArchiveExtract(request) => request.target_directory.display().to_string(),
        OperationPlan::RemoteDownload(request) => request.target_directory.display().to_string(),
        OperationPlan::Local(request) => request
            .target_directory
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "-".into()),
        OperationPlan::RemoteUpload(request) => {
            let profile = request.session.profile();
            format!(
                "{}@{}:{}",
                profile.auth.username(),
                profile.host,
                request.target_directory
            )
        }
    }
}

pub(crate) fn selected_paths_summary(paths: &[PathBuf]) -> String {
    if paths.len() == 1 {
        return paths[0].display().to_string();
    }

    t!("common.items_count", count = paths.len()).into_owned()
}
