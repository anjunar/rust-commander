use std::{fmt, path::PathBuf};

use crate::domain::PanelLocation;

#[derive(Clone, Debug)]
pub enum NavigationError {
    MissingArchiveSession {
        session_key: String,
    },
    MissingRemoteSession {
        session_key: String,
    },
    ReadFilesystem {
        path: PathBuf,
        detail: String,
    },
    ReadRemote {
        location: PanelLocation,
        detail: String,
    },
}

impl NavigationError {
    pub fn detail(&self) -> &str {
        match self {
            Self::MissingArchiveSession { .. } => "Archive session not found",
            Self::MissingRemoteSession { .. } => "Remote session not found",
            Self::ReadFilesystem { detail, .. } | Self::ReadRemote { detail, .. } => detail,
        }
    }
}

impl fmt::Display for NavigationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingArchiveSession { session_key } => {
                write!(f, "Archive session not found: {session_key}")
            }
            Self::MissingRemoteSession { session_key } => {
                write!(f, "Remote session not found: {session_key}")
            }
            Self::ReadFilesystem { path, detail } => {
                write!(f, "Could not read {}: {detail}", path.display())
            }
            Self::ReadRemote { location, detail } => {
                write!(f, "Could not read {}: {detail}", location.display_label())
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum OperationError {
    Planning { detail: String },
    Execution { detail: String },
}

impl OperationError {
    pub fn planning(detail: impl Into<String>) -> Self {
        Self::Planning {
            detail: detail.into(),
        }
    }

    pub fn execution(detail: impl Into<String>) -> Self {
        Self::Execution {
            detail: detail.into(),
        }
    }

    pub fn detail(&self) -> &str {
        match self {
            Self::Planning { detail } | Self::Execution { detail } => detail,
        }
    }
}

impl fmt::Display for OperationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Planning { detail } => write!(f, "Operation could not be prepared: {detail}"),
            Self::Execution { detail } => write!(f, "Operation failed: {detail}"),
        }
    }
}
