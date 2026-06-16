use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
};

use super::{
    ArchiveBackend, ArchiveCapabilities, ArchiveEntry, ArchiveEntryKind, ArchiveError,
    ArchiveFormatDetector, ArchiveSession, map_seven_zip_exit_code,
};

#[derive(Clone, Debug)]
pub struct SevenZipBackend {
    command_path: Arc<PathBuf>,
}

impl SevenZipBackend {
    pub fn new(command_path: impl Into<PathBuf>) -> Self {
        Self {
            command_path: Arc::new(command_path.into()),
        }
    }

    pub fn from_optional_path(configured_path: Option<PathBuf>) -> Self {
        Self::new(resolve_command_path(configured_path))
    }

    fn command_path(&self) -> &Path {
        self.command_path.as_path()
    }

    fn run_command(&self, args: &[OsString]) -> Result<std::process::Output, ArchiveError> {
        if !self.command_path().exists() {
            return Err(ArchiveError::BackendNotFound {
                backend: format!("7-Zip at {}", self.command_path().display()),
                path: None,
            });
        }

        Command::new(self.command_path())
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|error| ArchiveError::IoError {
                detail: format!("Could not start 7-Zip: {error}"),
            })
    }
}

impl ArchiveBackend for SevenZipBackend {
    fn id(&self) -> &'static str {
        "seven_zip"
    }

    fn display_name(&self) -> &'static str {
        "7-Zip"
    }

    fn capabilities(&self) -> ArchiveCapabilities {
        ArchiveCapabilities {
            list: true,
            extract_single: true,
            extract_all: true,
            test: true,
            password: true,
            progress: false,
            cancel: false,
            write_archive: false,
            delete_entry: false,
            rename_entry: false,
        }
    }

    fn can_open(&self, path: &Path) -> bool {
        ArchiveFormatDetector::is_supported_archive(path)
    }

    fn open(&self, path: &Path) -> Result<ArchiveSession, ArchiveError> {
        let args = vec![
            OsString::from("l"),
            OsString::from("-slt"),
            OsString::from("-ba"),
            path.as_os_str().to_os_string(),
        ];
        let output = self.run_command(&args)?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if !output.status.success() {
            return Err(map_seven_zip_exit_code(
                path.to_path_buf(),
                output.status.code(),
                combine_output(&stdout, &stderr),
            ));
        }

        let entries = parse_technical_listing(&stdout).map_err(|detail| ArchiveError::ListFailed {
            path: path.to_path_buf(),
            detail,
        })?;
        Ok(ArchiveSession::seven_zip(
            path.to_path_buf(),
            ArchiveFormatDetector::detect(path),
            entries,
            self.capabilities(),
        ))
    }

    fn list_entries(&self, session: &ArchiveSession) -> Result<Vec<ArchiveEntry>, ArchiveError> {
        if !matches!(session.session_kind(), super::ArchiveSessionKind::SevenZip) {
            return Err(ArchiveError::ListFailed {
                path: session.archive_path().to_path_buf(),
                detail: "Invalid archive session for 7-Zip backend".into(),
            });
        }
        Ok(session.cached_entries().to_vec())
    }

    fn extract_entry(
        &self,
        session: &ArchiveSession,
        entry_path: &str,
        target_dir: &Path,
    ) -> Result<(), ArchiveError> {
        let args = vec![
            OsString::from("x"),
            session.archive_path().as_os_str().to_os_string(),
            OsString::from(format!("-o{}", target_dir.display())),
            OsString::from("-y"),
            OsString::from(entry_path),
        ];
        let output = self.run_command(&args)?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if !output.status.success() {
            return Err(ArchiveError::ExtractionFailed {
                path: session.archive_path().to_path_buf(),
                detail: combine_output(&stdout, &stderr),
            });
        }
        Ok(())
    }

    fn extract_all(
        &self,
        session: &ArchiveSession,
        target_dir: &Path,
    ) -> Result<(), ArchiveError> {
        let args = vec![
            OsString::from("x"),
            session.archive_path().as_os_str().to_os_string(),
            OsString::from(format!("-o{}", target_dir.display())),
            OsString::from("-y"),
        ];
        let output = self.run_command(&args)?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if !output.status.success() {
            return Err(ArchiveError::ExtractionFailed {
                path: session.archive_path().to_path_buf(),
                detail: combine_output(&stdout, &stderr),
            });
        }
        Ok(())
    }

    fn test_archive(&self, session: &ArchiveSession) -> Result<(), ArchiveError> {
        let args = vec![
            OsString::from("t"),
            session.archive_path().as_os_str().to_os_string(),
        ];
        let output = self.run_command(&args)?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if !output.status.success() {
            return Err(map_seven_zip_exit_code(
                session.archive_path().to_path_buf(),
                output.status.code(),
                combine_output(&stdout, &stderr),
            ));
        }
        Ok(())
    }
}

pub fn parse_technical_listing(input: &str) -> Result<Vec<ArchiveEntry>, String> {
    let mut entries = Vec::new();
    let mut current = std::collections::BTreeMap::<String, String>::new();
    let mut in_files_section = false;

    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed == "----------" {
            in_files_section = true;
            continue;
        }

        if !in_files_section {
            continue;
        }

        if trimmed.is_empty() {
            if let Some(entry) = build_entry(&current)? {
                entries.push(entry);
            }
            current.clear();
            continue;
        }

        if let Some((key, value)) = line.split_once(" = ") {
            current.insert(key.trim().to_string(), value.trim().to_string());
        }
    }

    if let Some(entry) = build_entry(&current)? {
        entries.push(entry);
    }

    Ok(entries)
}

fn build_entry(fields: &std::collections::BTreeMap<String, String>) -> Result<Option<ArchiveEntry>, String> {
    if fields.is_empty() {
        return Ok(None);
    }

    let Some(path) = fields.get("Path").cloned() else {
        return Ok(None);
    };

    let normalized_path = path.replace('\\', "/").trim_matches('/').to_string();
    if normalized_path.is_empty() {
        return Ok(None);
    }

    let display_name = normalized_path
        .rsplit('/')
        .next()
        .unwrap_or(&normalized_path)
        .to_string();
    let attributes = fields.get("Attributes").cloned();
    let is_dir = attributes.as_deref().unwrap_or_default().contains('D')
        || fields.get("Folder").is_some_and(|value| value == "+");
    let kind = if is_dir {
        ArchiveEntryKind::Directory
    } else if attributes
        .as_deref()
        .unwrap_or_default()
        .contains('L')
    {
        ArchiveEntryKind::Symlink
    } else {
        ArchiveEntryKind::File
    };

    let size = fields
        .get("Size")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let packed_size = fields
        .get("Packed Size")
        .and_then(|value| value.parse::<u64>().ok());
    let modified_time = fields
        .get("Modified")
        .and_then(|value| chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").ok())
        .and_then(|value| value.and_local_timezone(chrono::Local).single())
        .map(Into::into);
    let encrypted = fields
        .get("Encrypted")
        .map(|value| value.eq_ignore_ascii_case("+") || value.eq_ignore_ascii_case("*"))
        .unwrap_or(false);

    Ok(Some(ArchiveEntry {
        archive_path: normalized_path,
        display_name,
        kind,
        size,
        packed_size,
        modified_time,
        crc: fields.get("CRC").cloned(),
        encrypted,
        method: fields.get("Method").cloned(),
        attributes,
    }))
}

fn combine_output(stdout: &str, stderr: &str) -> String {
    match (stdout.trim(), stderr.trim()) {
        ("", "") => "7-Zip returned an empty error output".into(),
        ("", stderr) => stderr.into(),
        (stdout, "") => stdout.into(),
        (stdout, stderr) => format!("{stdout}\n{stderr}"),
    }
}

fn resolve_command_path(configured_path: Option<PathBuf>) -> PathBuf {
    if let Some(path) = configured_path {
        return path;
    }

    let executable_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));

    #[cfg(target_os = "windows")]
    {
        executable_dir.join("tools").join("7zip").join("7z.exe")
    }

    #[cfg(not(target_os = "windows"))]
    {
        executable_dir.join("tools").join("7zip").join("7zz")
    }
}

#[cfg(test)]
mod tests {
    use super::parse_technical_listing;

    #[test]
    fn parses_technical_listing_fixture() {
        let fixture = include_str!("testdata/sevenzip-listing.txt");
        let entries = parse_technical_listing(fixture).unwrap();

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].archive_path, "docs");
        assert!(matches!(entries[0].kind, crate::archive::ArchiveEntryKind::Directory));
        assert_eq!(entries[1].archive_path, "docs/readme.txt");
        assert_eq!(entries[1].size, 42);
        assert!(entries[2].encrypted);
    }
}
