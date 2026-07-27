use std::time::Duration;

use rust_i18n::t;

use crate::{
    application::{
        start_operation_task, ActiveOperationHandle, ActivePanel, ConflictResolution,
        OperationEvent, OperationPlan, StartedOperation,
    },
    presentation,
    remote::RemotePath,
    ui::{dialogs, glib_poll},
};

use super::OperationsController;

impl OperationsController {
    pub(super) fn poll_transfer_operation(
        &self,
        request: OperationPlan,
        receiver: std::sync::mpsc::Receiver<OperationEvent>,
    ) {
        let title = t!(
            "progress.operation_title",
            kind = presentation::file_operation_label(&request.kind())
        )
        .into_owned();
        self.poll_transfer_operation_with_hooks(
            request,
            receiver,
            &title,
            false,
            false,
            None::<Box<dyn FnOnce(Result<(), String>)>>,
        );
    }

    /// Run a remote transfer with progress UI and an optional completion callback.
    ///
    /// When `auto_overwrite` is true, conflicts are resolved as overwrite without a dialog.
    /// Panel selection refresh is skipped (used for edit download/upload).
    pub fn run_remote_transfer_with_progress<F>(
        &self,
        request: OperationPlan,
        title: &str,
        auto_overwrite: bool,
        on_finished: F,
    ) where
        F: FnOnce(Result<(), String>) + 'static,
    {
        if self.runtime.active_operation.borrow().is_some() {
            on_finished(Err(
                t!("error.cancel_or_finish_current_operation").into_owned()
            ));
            return;
        }

        let started = match start_operation_task(
            self.task_spawner.clone(),
            &self.archive_service.borrow(),
            &self.remote_service,
            request.clone(),
        ) {
            Ok(started) => started,
            Err(error) => {
                on_finished(Err(error.detail().to_string()));
                return;
            }
        };

        let StartedOperation::Remote {
            handle,
            receiver,
            request,
        } = started
        else {
            on_finished(Err("Expected a remote transfer operation".into()));
            return;
        };

        self.runtime
            .active_operation
            .borrow_mut()
            .replace(ActiveOperationHandle::Remote(handle));
        self.poll_transfer_operation_with_hooks(
            request,
            receiver,
            title,
            auto_overwrite,
            true,
            Some(Box::new(on_finished)),
        );
    }

    pub(super) fn poll_transfer_operation_with_hooks(
        &self,
        request: OperationPlan,
        receiver: std::sync::mpsc::Receiver<OperationEvent>,
        title: &str,
        auto_overwrite: bool,
        skip_selection_refresh: bool,
        on_finished: Option<Box<dyn FnOnce(Result<(), String>)>>,
    ) {
        let active_operation = std::rc::Rc::clone(&self.runtime.active_operation);
        let progress_dialog = dialogs::ProgressDialog::new(&self.window, title, move || {
            if let Some(handle) = active_operation.borrow().as_ref() {
                handle.cancel();
            }
        });

        let on_finished = std::rc::Rc::new(std::cell::RefCell::new(on_finished));
        let controller = self.clone();
        glib_poll::poll_receiver(
            Duration::from_millis(80),
            receiver,
            move |event| match event {
                OperationEvent::Progress(snapshot) => {
                    progress_dialog.update_progress(&snapshot);
                    let update = {
                        let mut commander = controller.commander.borrow_mut();
                        commander.set_status(
                            t!(
                                "status.operation_current_item",
                                kind = presentation::file_operation_label(&snapshot.kind),
                                item = snapshot.current_item.as_str()
                            )
                            .into_owned(),
                        )
                    };
                    controller.host.apply_update(update);
                    true
                }
                OperationEvent::Conflict(conflict) => {
                    progress_dialog.set_waiting_for_conflict();
                    let should_overwrite = auto_overwrite
                        || !controller
                            .app_config_cache
                            .borrow()
                            .file_operations
                            .confirm_overwrite;
                    if should_overwrite {
                        if let Some(handle) = controller.runtime.active_operation.borrow().as_ref()
                        {
                            handle.resolve_conflict(ConflictResolution::Overwrite);
                        }
                        return true;
                    }
                    let handle = controller.runtime.active_operation.borrow().clone();
                    dialogs::show_conflict(&controller.window, conflict, move |resolution| {
                        if let Some(handle) = handle.as_ref() {
                            handle.resolve_conflict(resolution);
                        }
                    });
                    true
                }
                OperationEvent::Finished(summary) => {
                    progress_dialog.close();
                    controller.clear_active_operation();
                    if !skip_selection_refresh {
                        let selection_update = {
                            let mut commander = controller.commander.borrow_mut();
                            commander.queue_selection_after_file_operation(&request)
                        };
                        controller.host.apply_update(selection_update);
                    }
                    let status = t!(
                        "status.operation_completed",
                        kind = presentation::file_operation_label(&summary.kind),
                        count = summary.total_entries,
                        size = crate::fs::reader::format_bytes(summary.total_bytes),
                        seconds = format!("{:.1}", summary.elapsed.as_secs_f64())
                    )
                    .into_owned();
                    controller.host.set_status(status.clone());
                    if !skip_selection_refresh {
                        controller.finish_transfer_panel_refresh(&request, status);
                    }
                    if let Some(callback) = on_finished.borrow_mut().take() {
                        callback(Ok(()));
                    }
                    false
                }
                OperationEvent::Cancelled(summary) => {
                    progress_dialog.close();
                    controller.clear_active_operation();
                    if !skip_selection_refresh {
                        let selection_update = {
                            let mut commander = controller.commander.borrow_mut();
                            commander.queue_selection_after_file_operation(&request)
                        };
                        controller.host.apply_update(selection_update);
                    }
                    let status = t!(
                        "status.operation_cancelled",
                        kind = presentation::file_operation_label(&summary.kind),
                        count = summary.total_entries,
                        size = crate::fs::reader::format_bytes(summary.total_bytes)
                    )
                    .into_owned();
                    controller.host.set_status(status.clone());
                    if !skip_selection_refresh {
                        controller.finish_transfer_panel_refresh(&request, status);
                    }
                    if let Some(callback) = on_finished.borrow_mut().take() {
                        callback(Err(t!("status.remote_edit_transfer_cancelled").into_owned()));
                    }
                    false
                }
                OperationEvent::Failed(error) => {
                    progress_dialog.close();
                    controller.clear_active_operation();
                    let error_detail = error.detail().to_string();
                    let update = {
                        let mut commander = controller.commander.borrow_mut();
                        commander.set_status(
                            t!(
                                "status.file_operation_failed",
                                error = error_detail.as_str()
                            )
                            .into_owned(),
                        )
                    };
                    controller.host.apply_update(update);
                    if let Some(callback) = on_finished.borrow_mut().take() {
                        callback(Err(error_detail.clone()));
                    }
                    // Remote-edit transfers (`skip_selection_refresh`) handle errors in the callback.
                    if !skip_selection_refresh {
                        controller
                            .host
                            .show_error(&t!("error.file_operation_failed"), &error_detail);
                    }
                    false
                }
            },
            || false,
        );
    }

    /// After a transfer, drop stale SFTP listings and force-refresh both panes.
    /// Remote panels are not covered by the filesystem watcher, and
    /// `refresh_dirty_panels_if_idle` alone does nothing unless panels were queued.
    fn finish_transfer_panel_refresh(&self, request: &OperationPlan, status: String) {
        invalidate_remote_listing_cache(&self.remote_service, request);
        self.navigation
            .refresh_panels(&[ActivePanel::Left, ActivePanel::Right], status);
    }
}

fn invalidate_remote_listing_cache(
    remote_service: &crate::remote::RemoteService,
    plan: &OperationPlan,
) {
    match plan {
        OperationPlan::RemoteUpload(request) => {
            remote_service.invalidate_listing_cache_for_path(&request.target_directory);
        }
        OperationPlan::RemoteDownload(request) => {
            for entry_path in &request.entry_paths {
                if let Some(parent) = RemotePath::new(entry_path).parent() {
                    remote_service.invalidate_listing_cache_for_path(parent.as_str());
                } else {
                    remote_service.invalidate_listing_cache_for_path("/");
                }
            }
        }
        OperationPlan::Local(_) | OperationPlan::ArchiveExtract(_) => {}
    }
}
