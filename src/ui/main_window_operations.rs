use std::{rc::Rc, time::Duration};

use gtk::glib;
use rust_i18n::t;

use crate::{
    domain::{
        operation::{FileOperationKind, FileOperationRequest, OperationEvent},
        ConflictResolution,
    },
    presentation,
    ui::{
        dialogs,
        main_window::MainWindow,
        operations::{self, ActiveOperationHandle, PreparedOperation, StartedOperation},
    },
};

impl MainWindow {
    pub(super) fn handle_operation(self: &Rc<Self>, kind: FileOperationKind) {
        if self.active_operation.borrow().is_some() {
            dialogs::show_error(
                &self.window,
                &t!("error.file_operation_running"),
                &t!("error.cancel_or_finish_current_operation"),
            );
            return;
        }

        let prepared = match operations::prepare_operation(
            &self.commander.borrow(),
            &self.app_config_cache.borrow().file_operations,
            kind,
        ) {
            Ok(prepared) => prepared,
            Err(error) => {
                dialogs::show_error(
                    &self.window,
                    &t!("error.operation_unavailable"),
                    &error.to_string(),
                );
                return;
            }
        };

        match prepared {
            PreparedOperation::Start(request) => self.start_file_operation(request),
            PreparedOperation::Confirm(request) => {
                let this = Rc::clone(self);
                dialogs::confirm_operation(&self.window, request, move |request| {
                    this.start_file_operation(request);
                });
            }
        }
    }

    fn start_file_operation(self: &Rc<Self>, request: FileOperationRequest) {
        let started =
            match operations::start_operation_task(&self.archive_service.borrow(), request) {
                Ok(started) => started,
                Err(error) => {
                    dialogs::show_error(
                        &self.window,
                        &t!("error.operation_unavailable"),
                        &error.to_string(),
                    );
                    return;
                }
            };

        match started {
            StartedOperation::File {
                handle,
                receiver,
                request,
            } => {
                self.active_operation
                    .borrow_mut()
                    .replace(ActiveOperationHandle::File(handle.clone()));
                self.poll_file_operation(request, receiver);
            }
            StartedOperation::Archive { handle, receiver } => {
                self.active_operation
                    .borrow_mut()
                    .replace(ActiveOperationHandle::Archive(handle.clone()));
                self.poll_archive_extract_operation(receiver);
            }
        }
    }

    fn poll_file_operation(
        self: &Rc<Self>,
        request: FileOperationRequest,
        receiver: std::sync::mpsc::Receiver<OperationEvent>,
    ) {
        let active_operation = Rc::clone(&self.active_operation);
        let progress_dialog = dialogs::ProgressDialog::new(
            &self.window,
            &t!(
                "progress.operation_title",
                kind = presentation::file_operation_label(&request.kind)
            ),
            move || {
                if let Some(handle) = active_operation.borrow().as_ref() {
                    handle.cancel();
                }
            },
        );

        let this = Rc::clone(self);
        glib::timeout_add_local(Duration::from_millis(80), move || {
            let mut keep_running = true;

            while let Ok(event) = receiver.try_recv() {
                match event {
                    OperationEvent::Progress(snapshot) => {
                        progress_dialog.update_progress(&snapshot);
                        let update = {
                            let mut commander = this.commander.borrow_mut();
                            commander.set_status(
                                t!(
                                    "status.operation_current_item",
                                    kind = presentation::file_operation_label(&snapshot.kind),
                                    item = snapshot.current_item.as_str()
                                )
                                .into_owned(),
                            )
                        };
                        this.apply_update(update);
                    }
                    OperationEvent::Conflict(conflict) => {
                        progress_dialog.set_waiting_for_conflict();
                        if !this
                            .app_config_cache
                            .borrow()
                            .file_operations
                            .confirm_overwrite
                        {
                            if let Some(handle) = this.active_operation.borrow().as_ref() {
                                handle.resolve_conflict(ConflictResolution::Overwrite);
                            }
                            continue;
                        }
                        let handle = this.active_operation.borrow().clone();
                        dialogs::show_conflict(&this.window, conflict, move |resolution| {
                            if let Some(handle) = handle.as_ref() {
                                handle.resolve_conflict(resolution);
                            }
                        });
                    }
                    OperationEvent::Finished(summary) => {
                        progress_dialog.close();
                        this.active_operation.borrow_mut().take();
                        {
                            let mut commander = this.commander.borrow_mut();
                            commander.queue_selection_after_file_operation(&request);
                        }
                        let status = t!(
                            "status.operation_completed",
                            kind = presentation::file_operation_label(&summary.kind),
                            count = summary.total_entries,
                            size = crate::fs::reader::format_bytes(summary.total_bytes),
                            seconds = format!("{:.1}", summary.elapsed.as_secs_f64())
                        )
                        .into_owned();
                        this.set_status_message(status);
                        this.refresh_dirty_panels_if_idle();
                        keep_running = false;
                    }
                    OperationEvent::Cancelled(summary) => {
                        progress_dialog.close();
                        this.active_operation.borrow_mut().take();
                        {
                            let mut commander = this.commander.borrow_mut();
                            commander.queue_selection_after_file_operation(&request);
                        }
                        let status = t!(
                            "status.operation_cancelled",
                            kind = presentation::file_operation_label(&summary.kind),
                            count = summary.total_entries,
                            size = crate::fs::reader::format_bytes(summary.total_bytes)
                        )
                        .into_owned();
                        this.set_status_message(status);
                        this.refresh_dirty_panels_if_idle();
                        keep_running = false;
                    }
                    OperationEvent::Failed(error) => {
                        progress_dialog.close();
                        this.active_operation.borrow_mut().take();
                        let update = {
                            let mut commander = this.commander.borrow_mut();
                            commander.set_status(
                                t!("status.file_operation_failed", error = error.as_str())
                                    .into_owned(),
                            )
                        };
                        this.apply_update(update);
                        dialogs::show_error(
                            &this.window,
                            &t!("error.file_operation_failed"),
                            &error,
                        );
                        keep_running = false;
                    }
                }
            }

            if keep_running {
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
    }

    fn poll_archive_extract_operation(
        self: &Rc<Self>,
        receiver: std::sync::mpsc::Receiver<crate::archive::ArchiveTaskEvent>,
    ) {
        let active_operation = Rc::clone(&self.active_operation);
        let progress_dialog =
            dialogs::ProgressDialog::new(&self.window, &t!("progress.archive_copy"), move || {
                if let Some(handle) = active_operation.borrow().as_ref() {
                    handle.cancel();
                }
            });

        let this = Rc::clone(self);
        glib::timeout_add_local(Duration::from_millis(80), move || {
            let mut keep_running = true;

            while let Ok(event) = receiver.try_recv() {
                match event {
                    crate::archive::ArchiveTaskEvent::Progress(progress) => {
                        progress_dialog.update_archive_progress(&progress);
                        let update = {
                            let mut commander = this.commander.borrow_mut();
                            commander.set_status(
                                t!(
                                    "status.copy_current_path",
                                    path = progress.current_path.clone().unwrap_or_else(|| t!(
                                        "status.archive_extraction_in_progress"
                                    )
                                    .into_owned())
                                )
                                .into_owned(),
                            )
                        };
                        this.apply_update(update);
                    }
                    crate::archive::ArchiveTaskEvent::Finished(message) => {
                        progress_dialog.close();
                        this.active_operation.borrow_mut().take();
                        this.set_status_message(message);
                        this.refresh_dirty_panels_if_idle();
                        keep_running = false;
                    }
                    crate::archive::ArchiveTaskEvent::Cancelled => {
                        progress_dialog.close();
                        this.active_operation.borrow_mut().take();
                        this.set_status_message(t!("status.archive_copy_cancelled").into_owned());
                        this.refresh_dirty_panels_if_idle();
                        keep_running = false;
                    }
                    crate::archive::ArchiveTaskEvent::Failed(error) => {
                        progress_dialog.close();
                        this.active_operation.borrow_mut().take();
                        let update = {
                            let mut commander = this.commander.borrow_mut();
                            commander.set_status(
                                t!("status.archive_copy_failed", error = error.to_string())
                                    .into_owned(),
                            )
                        };
                        this.apply_update(update);
                        dialogs::show_error(
                            &this.window,
                            &t!("error.archive_copy_failed"),
                            &error.to_string(),
                        );
                        keep_running = false;
                    }
                }
            }

            if keep_running {
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
    }
}
