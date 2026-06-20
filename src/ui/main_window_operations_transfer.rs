use std::time::Duration;

use rust_i18n::t;

use crate::{
    application::{ConflictResolution, OperationEvent, OperationPlan},
    presentation,
    ui::{dialogs, glib_poll},
};

use super::OperationsController;

impl OperationsController {
    pub(super) fn poll_transfer_operation(
        &self,
        request: OperationPlan,
        receiver: std::sync::mpsc::Receiver<OperationEvent>,
    ) {
        let active_operation = std::rc::Rc::clone(&self.runtime.active_operation);
        let progress_dialog = dialogs::ProgressDialog::new(
            &self.window,
            &t!(
                "progress.operation_title",
                kind = presentation::file_operation_label(&request.kind())
            ),
            move || {
                if let Some(handle) = active_operation.borrow().as_ref() {
                    handle.cancel();
                }
            },
        );

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
                    if !controller
                        .app_config_cache
                        .borrow()
                        .file_operations
                        .confirm_overwrite
                    {
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
                    {
                        let mut commander = controller.commander.borrow_mut();
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
                    controller.host.set_status(status);
                    controller.navigation.refresh_dirty_panels_if_idle();
                    false
                }
                OperationEvent::Cancelled(summary) => {
                    progress_dialog.close();
                    controller.clear_active_operation();
                    {
                        let mut commander = controller.commander.borrow_mut();
                        commander.queue_selection_after_file_operation(&request);
                    }
                    let status = t!(
                        "status.operation_cancelled",
                        kind = presentation::file_operation_label(&summary.kind),
                        count = summary.total_entries,
                        size = crate::fs::reader::format_bytes(summary.total_bytes)
                    )
                    .into_owned();
                    controller.host.set_status(status);
                    controller.navigation.refresh_dirty_panels_if_idle();
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
                    controller
                        .host
                        .show_error(&t!("error.file_operation_failed"), &error_detail);
                    false
                }
            },
            || false,
        );
    }
}
