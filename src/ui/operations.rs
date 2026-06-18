use std::sync::mpsc::Receiver;

use anyhow::{bail, Result};

use crate::{
    application::Commander,
    archive::{ArchiveService, ArchiveTaskEvent, ArchiveTaskHandle, ArchiveTaskRequest},
    config::FileOperationsConfig,
    domain::operation::{ConflictResolution, FileOperationKind, FileOperationRequest, OperationEvent},
    fs::operations::{start_operation, OperationHandle},
};

#[derive(Clone)]
pub enum ActiveOperationHandle {
    File(OperationHandle),
    Archive(ArchiveTaskHandle),
}

impl ActiveOperationHandle {
    pub fn cancel(&self) {
        match self {
            Self::File(handle) => handle.cancel(),
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
    Start(FileOperationRequest),
    Confirm(FileOperationRequest),
}

impl PreparedOperation {
    pub fn request(self) -> FileOperationRequest {
        match self {
            Self::Start(request) | Self::Confirm(request) => request,
        }
    }
}

pub enum StartedOperation {
    File {
        handle: OperationHandle,
        receiver: Receiver<OperationEvent>,
        request: FileOperationRequest,
    },
    Archive {
        handle: ArchiveTaskHandle,
        receiver: Receiver<ArchiveTaskEvent>,
    },
}

pub fn prepare_operation(
    commander: &Commander,
    file_operations: &FileOperationsConfig,
    kind: FileOperationKind,
) -> Result<PreparedOperation> {
    let is_delete = matches!(kind, FileOperationKind::Delete);
    let request = commander.operation_request(kind)?;

    if is_delete && !file_operations.confirm_delete {
        Ok(PreparedOperation::Start(request))
    } else {
        Ok(PreparedOperation::Confirm(request))
    }
}

pub fn start_operation_task(
    archive_service: &ArchiveService,
    request: FileOperationRequest,
) -> Result<StartedOperation> {
    if let Some(archive_source) = request.archive_source.clone() {
        let Some(target_dir) = request.target_directory.clone() else {
            bail!("No filesystem target directory available for archive copy");
        };

        let (handle, receiver) = archive_service.start_task(ArchiveTaskRequest::ExtractSelection {
            session: archive_source.session,
            entry_paths: archive_source.entry_paths,
            target_dir,
        });

        Ok(StartedOperation::Archive { handle, receiver })
    } else {
        let (handle, receiver) = start_operation(request.clone());
        Ok(StartedOperation::File {
            handle,
            receiver,
            request,
        })
    }
}
