use std::{fmt, path::PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArchiveError {
    BackendNotFound {
        backend: String,
        path: Option<PathBuf>,
    },
    UnsupportedFormat {
        path: PathBuf,
    },
    InvalidArchive {
        path: PathBuf,
        detail: Option<String>,
    },
    PasswordRequired {
        path: PathBuf,
    },
    WrongPassword {
        path: PathBuf,
    },
    ExtractionFailed {
        path: PathBuf,
        detail: String,
    },
    ListFailed {
        path: PathBuf,
        detail: String,
    },
    UnsafeArchivePath {
        archive_path: String,
    },
    Cancelled,
    IoError {
        detail: String,
    },
    ProcessError {
        command: String,
        exit_code: Option<i32>,
        detail: String,
    },
    LibraryError {
        library: String,
        detail: String,
    },
    FeatureNotSupported {
        backend: String,
        feature: String,
    },
}

impl std::error::Error for ArchiveError {}

impl fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BackendNotFound { backend, .. } => {
                write!(f, "Archive backend not found: {backend}")
            }
            Self::UnsupportedFormat { path } => {
                write!(f, "Unsupported archive format: {}", path.display())
            }
            Self::InvalidArchive { path, .. } => write!(f, "Invalid archive: {}", path.display()),
            Self::PasswordRequired { path } => {
                write!(f, "Password required for {}", path.display())
            }
            Self::WrongPassword { path } => write!(f, "Wrong password for {}", path.display()),
            Self::ExtractionFailed { path, detail } => {
                write!(f, "Extraction failed for {}: {detail}", path.display())
            }
            Self::ListFailed { path, detail } => {
                write!(f, "Could not list {}: {detail}", path.display())
            }
            Self::UnsafeArchivePath { archive_path } => {
                write!(f, "Unsafe archive path: {archive_path}")
            }
            Self::Cancelled => write!(f, "Archive operation cancelled"),
            Self::IoError { detail } => write!(f, "{detail}"),
            Self::ProcessError {
                command,
                exit_code,
                detail,
            } => {
                write!(f, "Process failed ({command}, {:?}): {detail}", exit_code)
            }
            Self::LibraryError { library, detail } => {
                write!(f, "Library backend failed ({library}): {detail}")
            }
            Self::FeatureNotSupported { backend, feature } => {
                write!(f, "Feature not supported by {backend}: {feature}")
            }
        }
    }
}
