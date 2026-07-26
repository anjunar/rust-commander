use std::{cell::RefCell, rc::Rc};

use rust_i18n::t;

use crate::{
    application::{
        prepare_operation_with_options, start_operation_task, ActiveOperationHandle, Commander,
        DeleteMode, FileOperationKind, OperationPlan, PreparedOperation, SessionStore,
        StartedOperation, TaskSpawner,
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

pub struct OperationsControllerDeps {
    pub host: Rc<dyn OperationsHost>,
    pub window: gtk::ApplicationWindow,
    pub commander: Rc<RefCell<Commander>>,
    pub archive_service: Rc<RefCell<ArchiveService>>,
    pub remote_service: RemoteService,
    pub session_store: Rc<RefCell<SessionStore>>,
    pub task_spawner: TaskSpawner,
    pub runtime: OperationRuntime,
    pub app_config_cache: Rc<RefCell<AppConfig>>,
    pub navigation: NavigationController,
}

impl OperationsController {
    pub fn new(deps: OperationsControllerDeps) -> Self {
        Self {
            host: deps.host,
            window: deps.window,
            commander: deps.commander,
            archive_service: deps.archive_service,
            remote_service: deps.remote_service,
            session_store: deps.session_store,
            task_spawner: deps.task_spawner,
            runtime: deps.runtime,
            app_config_cache: deps.app_config_cache,
            navigation: deps.navigation,
        }
    }

    pub fn handle_operation(&self, kind: FileOperationKind) {
        self.handle_operation_with_delete_mode(kind, DeleteMode::Default);
    }

    pub fn handle_delete_permanent(&self) {
        self.handle_operation_with_delete_mode(FileOperationKind::Delete, DeleteMode::Permanent);
    }

    /// Start a prepared plan immediately (clipboard paste — no confirmation).
    pub fn start_prepared_operation<F>(&self, request: OperationPlan, on_finished: F)
    where
        F: FnOnce(Result<(), String>) + 'static,
    {
        if self.runtime.active_operation.borrow().is_some() {
            dialogs::show_error(
                &self.window,
                &t!("error.file_operation_running"),
                &t!("error.cancel_or_finish_current_operation"),
            );
            return;
        }

        self.start_file_operation_with_finish(request, on_finished);
    }

    fn handle_operation_with_delete_mode(&self, kind: FileOperationKind, delete_mode: DeleteMode) {
        if self.runtime.active_operation.borrow().is_some() {
            dialogs::show_error(
                &self.window,
                &t!("error.file_operation_running"),
                &t!("error.cancel_or_finish_current_operation"),
            );
            return;
        }

        let prepared = match prepare_operation_with_options(
            &self.commander.borrow(),
            Rc::clone(&self.session_store),
            &self.app_config_cache.borrow().file_operations,
            kind,
            delete_mode,
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

    fn start_file_operation_with_finish<F>(&self, request: OperationPlan, on_finished: F)
    where
        F: FnOnce(Result<(), String>) + 'static,
    {
        let started = match start_operation_task(
            self.task_spawner.clone(),
            &self.archive_service.borrow(),
            &self.remote_service,
            request,
        ) {
            Ok(started) => started,
            Err(error) => {
                let detail = error.detail().to_string();
                dialogs::show_error(
                    &self.window,
                    &t!("error.operation_unavailable"),
                    &detail,
                );
                on_finished(Err(detail));
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
                let title = t!(
                    "progress.operation_title",
                    kind = crate::presentation::file_operation_label(&request.kind())
                )
                .into_owned();
                self.poll_transfer_operation_with_hooks(
                    request,
                    receiver,
                    &title,
                    false,
                    false,
                    Some(Box::new(on_finished)),
                );
            }
            StartedOperation::Archive { handle, receiver } => {
                self.runtime
                    .active_operation
                    .borrow_mut()
                    .replace(ActiveOperationHandle::Archive(handle.clone()));
                // Clipboard paste never starts archive extract.
                let _ = on_finished;
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
                let title = t!(
                    "progress.operation_title",
                    kind = crate::presentation::file_operation_label(&request.kind())
                )
                .into_owned();
                self.poll_transfer_operation_with_hooks(
                    request,
                    receiver,
                    &title,
                    false,
                    false,
                    Some(Box::new(on_finished)),
                );
            }
        }
    }

    fn clear_active_operation(&self) {
        self.runtime.active_operation.borrow_mut().take();
    }
}
