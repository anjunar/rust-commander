use std::path::PathBuf;

use gtk::{glib, prelude::*};
use rust_i18n::t;

use crate::{
    application::{ConflictResolution, FileOperationKind, OperationConflict, OperationPlan},
    fs::{
        operations::{format_eta, progress_percent},
        reader::format_bytes,
    },
    presentation,
    ui::dsl::{button, progress, text, Property},
};

use super::dialogs_base::{heading, modal, paragraph, Once};

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

    // The operation starts on the next main-loop turn, once the dialog is gone.
    let on_confirm = Once::new(move |()| {
        glib::idle_add_local_once(move || on_confirm(request));
    });

    let (window, confirm) = modal(parent, &title, (480, 160), |dialog| {
        dialog.content(|| {
            heading(title.as_str());
            paragraph(detail);
        });
        dialog.actions(|| {
            dialog.close_button(t!("common.cancel"));
            button(confirm_label, |button| {
                button.css("suggested-action");
                let window = dialog.window().widget().clone();
                let on_confirm = on_confirm.clone();
                button.on_click(move || {
                    window.close();
                    on_confirm.call(());
                });
            })
        })
    });
    window.set_default_widget(Some(confirm.widget()));

    window.present();
}

pub fn show_conflict<F>(
    parent: &gtk::ApplicationWindow,
    conflict: OperationConflict,
    on_resolution: F,
) where
    F: FnOnce(ConflictResolution) + 'static,
{
    let detail = t!(
        "dialog.conflict_detail",
        source = conflict.source.display().to_string(),
        target = conflict.target.display().to_string()
    )
    .into_owned();
    let title = t!(
        "dialog.conflict_title",
        kind = presentation::file_operation_label(&conflict.kind)
    )
    .into_owned();
    let on_resolution = Once::new(on_resolution);

    let (window, skip) = modal(parent, &title, (520, 240), |dialog| {
        let resolve = |resolution: ConflictResolution| {
            let on_resolution = on_resolution.clone();
            let window = dialog.window().widget().clone();
            move || {
                on_resolution.call(resolution.clone());
                window.close();
            }
        };

        dialog.content(|| {
            heading(title.as_str());
            paragraph(detail);
        });
        dialog.actions(|| {
            button(t!("common.cancel"), |button| {
                button.on_click(resolve(ConflictResolution::Cancel));
            });
            let skip = button(t!("common.skip"), |button| {
                button.on_click(resolve(ConflictResolution::Skip));
            });
            button(t!("common.rename"), |button| {
                button.on_click(resolve(ConflictResolution::Rename));
            });
            button(t!("common.overwrite"), |button| {
                button.css("suggested-action");
                button.on_click(resolve(ConflictResolution::Overwrite));
            });
            skip
        })
    });
    window.set_default_widget(Some(skip.widget()));

    window.present();
}

/// A modal progress window. The text and the bar follow the properties; the
/// bar itself is kept only for `pulse`, which is an action and not state.
#[derive(Clone)]
pub struct ProgressDialog {
    window: gtk::Window,
    bar: gtk::ProgressBar,
    title: Property<String>,
    detail: Property<String>,
    eta: Property<String>,
    fraction: Property<f64>,
    progress_text: Property<String>,
}

impl ProgressDialog {
    pub fn new<F>(parent: &gtk::ApplicationWindow, title_text: &str, on_cancel: F) -> Self
    where
        F: Fn() + 'static,
    {
        let title = Property::new(title_text.to_string());
        let detail = Property::new(t!("progress.preparing_file_operation").into_owned());
        let eta = Property::new(t!("progress.eta_unknown").into_owned());
        let fraction = Property::new(0.0);
        let progress_text = Property::new(percent_text(0.0));

        let (window, bar) = modal(parent, title_text, (460, 160), |dialog| {
            let bar = dialog.content(|| {
                heading(&title);
                paragraph(&detail);
                let bar = progress(&fraction, |bar| bar.bind_progress_text(&progress_text));
                text(&eta, |_| {});
                bar
            });
            dialog.actions(|| {
                button(t!("progress.cancel_operation"), |button| {
                    let window = dialog.window().widget().clone();
                    button.on_click(move || {
                        on_cancel();
                        window.set_sensitive(false);
                    });
                });
            });
            bar
        });

        window.present();

        Self {
            window,
            bar: bar.widget().clone(),
            title,
            detail,
            eta,
            fraction,
            progress_text,
        }
    }

    pub fn update_progress(&self, snapshot: &crate::application::OperationSnapshot) {
        self.title.set(
            t!(
                "progress.in_progress",
                kind = presentation::file_operation_label(&snapshot.kind)
            )
            .into_owned(),
        );
        self.detail.set(
            t!(
                "progress.file_operation_detail",
                processed = format_bytes(snapshot.processed_bytes),
                total = format_bytes(snapshot.total_bytes),
                processed_entries = snapshot.processed_entries,
                total_entries = snapshot.total_entries,
                item = snapshot.current_item.as_str()
            )
            .into_owned(),
        );
        self.set_fraction(progress_percent(snapshot));
        self.eta.set(format_eta(snapshot));
    }

    pub fn update_archive_progress(&self, progress: &crate::archive::ArchiveProgress) {
        self.title
            .set(t!("progress.archive_copy_in_progress").into_owned());

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
        self.detail.set(detail);

        if let Some(percent) = progress.percent {
            self.set_fraction(percent.clamp(0.0, 1.0));
        } else {
            self.bar.pulse();
            self.progress_text.set(t!("progress.working").into_owned());
        }

        let processed_entries = progress.processed_entries.unwrap_or(0);
        let total_entries = progress.total_entries.unwrap_or(0);
        self.eta.set(
            t!(
                "progress.items_count",
                processed = processed_entries,
                total = total_entries
            )
            .into_owned(),
        );
    }

    pub fn set_waiting_for_conflict(&self) {
        self.detail
            .set(t!("progress.waiting_for_conflict_resolution").into_owned());
        self.bar.pulse();
    }

    pub fn close(&self) {
        self.window.close();
    }

    fn set_fraction(&self, fraction: f64) {
        self.fraction.set(fraction);
        self.progress_text.set(percent_text(fraction));
    }
}

fn percent_text(fraction: f64) -> String {
    format!("{:.0}%", fraction * 100.0)
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
