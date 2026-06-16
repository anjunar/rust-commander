use std::{fmt, path::PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArchiveError {
    BackendNotFound { backend: String, path: Option<PathBuf> },
    UnsupportedFormat { path: PathBuf },
    InvalidArchive { path: PathBuf, detail: Option<String> },
    EncryptedArchivePasswordRequired { path: PathBuf },
    WrongPassword { path: PathBuf },
    ExtractionFailed { path: PathBuf, detail: String },
    ListFailed { path: PathBuf, detail: String },
    Cancelled,
    UnsafeArchivePath { archive_path: String },
    IoError { detail: String },
    ProcessError { command: String, exit_code: Option<i32>, detail: String },
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
            Self::EncryptedArchivePasswordRequired { path } => {
                write!(f, "Password required for {}", path.display())
            }
            Self::WrongPassword { path } => write!(f, "Wrong password for {}", path.display()),
            Self::ExtractionFailed { path, detail } => {
                write!(f, "Extraction failed for {}: {detail}", path.display())
            }
            Self::ListFailed { path, detail } => {
                write!(f, "Could not list {}: {detail}", path.display())
            }
            Self::Cancelled => write!(f, "Archive operation cancelled"),
            Self::UnsafeArchivePath { archive_path } => {
                write!(f, "Unsafe archive path: {archive_path}")
            }
            Self::IoError { detail } => write!(f, "{detail}"),
            Self::ProcessError {
                command,
                exit_code,
                detail,
            } => {
                write!(f, "Process failed ({command}, {:?}): {detail}", exit_code)
            }
        }
    }
}

pub fn map_seven_zip_exit_code(
    archive_path: PathBuf,
    exit_code: Option<i32>,
    detail: String,
) -> ArchiveError {
    let detail_lower = detail.to_ascii_lowercase();

    match exit_code {
        Some(255) => ArchiveError::Cancelled,
        Some(2) if detail_lower.contains("wrong password") => {
            ArchiveError::WrongPassword { path: archive_path }
        }
        Some(2) if detail_lower.contains("can not open encrypted archive")
            || detail_lower.contains("password") =>
        {
            ArchiveError::EncryptedArchivePasswordRequired { path: archive_path }
        }
        Some(2) => ArchiveError::InvalidArchive {
            path: archive_path,
            detail: Some(detail),
        },
        Some(1) => ArchiveError::ProcessError {
            command: "7z".into(),
            exit_code,
            detail,
        },
        Some(_) | None => ArchiveError::ProcessError {
            command: "7z".into(),
            exit_code,
            detail,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{ArchiveError, map_seven_zip_exit_code};

    #[test]
    fn maps_password_error_from_seven_zip() {
        let error = map_seven_zip_exit_code(
            std::path::PathBuf::from("secret.zip"),
            Some(2),
            "ERROR: Wrong password?".into(),
        );

        assert!(matches!(error, ArchiveError::WrongPassword { .. }));
    }
}
