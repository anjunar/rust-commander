use std::{cell::RefCell, rc::Rc};

use rust_i18n::t;

use crate::{
    application::{
        prepare_operation, start_operation_task, ActiveOperationHandle, Commander,
        FileOperationKind, OperationPlan, PreparedOperation, SessionStore, StartedOperation,
        TaskSpawner,
    },
    archive::ArchiveService,
    config::AppConfig,
    remote::RemoteService,
    ui::dialogs,
};

use super::{hosts::OperationsHost, navigation_controller::NavigationController};

#[path = "main_window_operations_archive.rs"]
mod archive_poll;
#[path = "main_window_operations_transfer.rs"]
mod transfer_poll;

#[derive(Clone)]
pub struct OperationRuntime {
    pub active_operation: Rc<RefCell<Option<ActiveOperationHandle>>>,
}

impl OperationRuntime {
    pub fn new() -> Self {
        Self {
            active_operation: Rc::new(RefCell::new(None)),
        }
    }
}

#[derive(Clone)]
pub struct OperationsController {
    host: Rc<dyn OperationsHost>,
    window: gtk::ApplicationWindow,
    commander: Rc<RefCell<Commander>>,
    archive_service: Rc<RefCell<ArchiveService>>,
    remote_service: RemoteService,
    session_store: Rc<RefCell<SessionStore>>,
    task_spawner: TaskSpawner,
    runtime: OperationRuntime,
    app_config_cache: Rc<RefCell<AppConfig>>,
    navigation: NavigationController,
}

impl OperationsController {
    pub fn new(
        host: Rc<dyn OperationsHost>,
        window: gtk::ApplicationWindow,
        commander: Rc<RefCell<Commander>>,
        archive_service: Rc<RefCell<ArchiveService>>,
        remote_service: RemoteService,
        session_store: Rc<RefCell<SessionStore>>,
        task_spawner: TaskSpawner,
        runtime: OperationRuntime,
        app_config_cache: Rc<RefCell<AppConfig>>,
        navigation: NavigationController,
    ) -> Self {
        Self {
            host,
            window,
            commander,
            archive_service,
            remote_service,
            session_store,
            task_spawner,
            runtime,
            app_config_cache,
            navigation,
        }
    }

    pub fn handle_operation(&self, kind: FileOperationKind) {
        if self.runtime.active_operation.borrow().is_some() {
            dialogs::show_error(
                &self.window,
                &t!("error.file_operation_running"),
                &t!("error.cancel_or_finish_current_operation"),
            );
            return;
        }

        let prepared = match prepare_operation(
            &self.commander.borrow(),
            Rc::clone(&self.session_store),
            &self.app_config_cache.borrow().file_operations,
            kind,
        ) {
            Ok(prepared) => prepared,
            Err(error) => {
                dialogs::show_error(
                    &self.window,
                    &t!("error.operation_unavailable"),
                    error.detail(),
                );
                return;
            }
        };

        match prepared {
            PreparedOperation::Start(request) => self.start_file_operation(request),
            PreparedOperation::Confirm(request) => {
                let controller = self.clone();
                dialogs::confirm_operation(&self.window, request, move |request| {
                    controller.start_file_operation(request);
                });
            }
        }
    }

    fn start_file_operation(&self, request: OperationPlan) {
        let started = match start_operation_task(
            self.task_spawner.clone(),
            &self.archive_service.borrow(),
            &self.remote_service,
            request,
        ) {
            Ok(started) => started,
            Err(error) => {
                dialogs::show_error(
                    &self.window,
                    &t!("error.operation_unavailable"),
                    error.detail(),
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
                self.runtime
                    .active_operation
                    .borrow_mut()
                    .replace(ActiveOperationHandle::File(handle.clone()));
                self.poll_transfer_operation(request, receiver);
            }
            StartedOperation::Archive { handle, receiver } => {
                self.runtime
                    .active_operation
                    .borrow_mut()
                    .replace(ActiveOperationHandle::Archive(handle.clone()));
                self.poll_archive_extract_operation(receiver);
            }
            StartedOperation::Remote {
                handle,
                receiver,
                request,
            } => {
                self.runtime
                    .active_operation
                    .borrow_mut()
                    .replace(ActiveOperationHandle::Remote(handle.clone()));
                self.poll_transfer_operation(request, receiver);
            }
        }
    }

    fn clear_active_operation(&self) {
        self.runtime.active_operation.borrow_mut().take();
    }
}
