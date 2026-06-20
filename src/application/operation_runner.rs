use std::{cell::RefCell, rc::Rc, sync::mpsc::Receiver};

use anyhow::Result;

use crate::{
    archive::{ArchiveService, ArchiveTaskEvent, ArchiveTaskHandle, ArchiveTaskRequest},
    config::FileOperationsConfig,
    fs::operations::{start_operation, OperationHandle},
    remote::{RemoteOperationHandle, RemoteService},
};

use super::{
    ArchiveExtractRequest, Commander, ConflictResolution, FileOperationKind, OperationEvent,
    OperationPlan, RemoteDownloadRequest, RemoteUploadRequest, SessionStore,
};

#[derive(Clone)]
pub enum ActiveOperationHandle {
    File(OperationHandle),
    Remote(RemoteOperationHandle),
    Archive(ArchiveTaskHandle),
}

impl ActiveOperationHandle {
    pub fn cancel(&self) {
        match self {
            Self::File(handle) => handle.cancel(),
            Self::Remote(handle) => handle.cancel(),
            Self::Archive(handle) => handle.cancel(),
        }
    }

    pub fn resolve_conflict(&self, resolution: ConflictResolution) {
        if let Self::File(handle) = self {
            handle.resolve_conflict(resolution);
        }
    }
}

pub enum PreparedOperation {
    Start(OperationPlan),
    Confirm(OperationPlan),
}

pub enum StartedOperation {
    File {
        handle: OperationHandle,
        receiver: Receiver<OperationEvent>,
        request: OperationPlan,
    },
    Remote {
        handle: RemoteOperationHandle,
        receiver: Receiver<OperationEvent>,
        request: OperationPlan,
    },
    Archive {
        handle: ArchiveTaskHandle,
        receiver: Receiver<ArchiveTaskEvent>,
    },
}

pub fn prepare_operation(
    commander: &Commander,
    session_store: Rc<RefCell<SessionStore>>,
    file_operations: &FileOperationsConfig,
    kind: FileOperationKind,
) -> Result<PreparedOperation> {
    let is_delete = matches!(kind, FileOperationKind::Delete);
    let request = commander
        .operation_request(kind, &session_store.borrow())?
        .with_use_recycle_bin(is_delete && file_operations.use_recycle_bin);

    if is_delete && !file_operations.confirm_delete {
        Ok(PreparedOperation::Start(request))
    } else {
        Ok(PreparedOperation::Confirm(request))
    }
}

pub fn start_operation_task(
    archive_service: &ArchiveService,
    remote_service: &RemoteService,
    request: OperationPlan,
) -> Result<StartedOperation> {
    match request.clone() {
        OperationPlan::ArchiveExtract(ArchiveExtractRequest {
            session,
            entry_paths,
            target_directory,
        }) => {
            let (handle, receiver) =
                archive_service.start_task(ArchiveTaskRequest::ExtractSelection {
                    session,
                    entry_paths,
                    target_dir: target_directory,
                });

            Ok(StartedOperation::Archive { handle, receiver })
        }
        OperationPlan::RemoteDownload(RemoteDownloadRequest {
            session,
            entry_paths,
            target_directory,
        }) => {
            let (handle, receiver) = remote_service.start_download(RemoteDownloadRequest {
                session,
                entry_paths,
                target_directory,
            });
            Ok(StartedOperation::Remote {
                handle,
                receiver,
                request,
            })
        }
        OperationPlan::RemoteUpload(RemoteUploadRequest {
            sources,
            session,
            target_directory,
        }) => {
            let (handle, receiver) = remote_service.start_upload(RemoteUploadRequest {
                sources: sources.clone(),
                session,
                target_directory,
            });
            Ok(StartedOperation::Remote {
                handle,
                receiver,
                request,
            })
        }
        OperationPlan::Local(request) => {
            let request_for_tracking = request.clone();
            let (handle, receiver) = start_operation(request);
            Ok(StartedOperation::File {
                handle,
                receiver,
                request: OperationPlan::Local(request_for_tracking),
            })
        }
    }
}
