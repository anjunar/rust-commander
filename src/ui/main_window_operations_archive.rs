use std::time::Duration;

use rust_i18n::t;

use crate::ui::{dialogs, glib_poll};

use super::OperationsController;

impl OperationsController {
    pub(super) fn poll_archive_extract_operation(
        &self,
        receiver: std::sync::mpsc::Receiver<crate::archive::ArchiveTaskEvent>,
    ) {
        let active_operation = std::rc::Rc::clone(&self.runtime.active_operation);
        let progress_dialog =
            dialogs::ProgressDialog::new(&self.window, &t!("progress.archive_copy"), move || {
                if let Some(handle) = active_operation.borrow().as_ref() {
                    handle.cancel();
                }
            });

        let controller = self.clone();
        glib_poll::poll_receiver(
            Duration::from_millis(80),
            receiver,
            move |event| match event {
                crate::archive::ArchiveTaskEvent::Progress(progress) => {
                    progress_dialog.update_archive_progress(&progress);
                    let update = {
                        let mut commander = controller.commander.borrow_mut();
                        commander.set_status(
                            t!(
                                "status.copy_current_path",
                                path =
                                    progress.current_path.clone().unwrap_or_else(|| t!(
                                        "status.archive_extraction_in_progress"
                                    )
                                    .into_owned())
                            )
                            .into_owned(),
                        )
                    };
                    controller.host.apply_update(update);
                    true
                }
                crate::archive::ArchiveTaskEvent::Finished(message) => {
                    progress_dialog.close();
                    controller.clear_active_operation();
                    controller.host.set_status(message);
                    controller.navigation.refresh_dirty_panels_if_idle();
                    false
                }
                crate::archive::ArchiveTaskEvent::Cancelled => {
                    progress_dialog.close();
                    controller.clear_active_operation();
                    controller
                        .host
                        .set_status(t!("status.archive_copy_cancelled").into_owned());
                    controller.navigation.refresh_dirty_panels_if_idle();
                    false
                }
                crate::archive::ArchiveTaskEvent::Failed(error) => {
                    progress_dialog.close();
                    controller.clear_active_operation();
                    let update = {
                        let mut commander = controller.commander.borrow_mut();
                        commander.set_status(
                            t!("status.archive_copy_failed", error = error.to_string())
                                .into_owned(),
                        )
                    };
                    controller.host.apply_update(update);
                    controller
                        .host
                        .show_error(&t!("error.archive_copy_failed"), &error.to_string());
                    false
                }
            },
            || false,
        );
    }
}
