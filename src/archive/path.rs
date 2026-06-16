use std::path::{Component, Path, PathBuf};

use super::ArchiveError;

pub fn safe_join_extract_path(
    target_dir: &Path,
    archive_entry_path: &str,
) -> Result<PathBuf, ArchiveError> {
    if archive_entry_path.is_empty() {
        return Err(ArchiveError::UnsafeArchivePath {
            archive_path: archive_entry_path.into(),
        });
    }

    let normalized = archive_entry_path.replace('\\', "/");
    if normalized.starts_with('/') {
        return Err(ArchiveError::UnsafeArchivePath {
            archive_path: archive_entry_path.into(),
        });
    }

    if has_windows_drive_prefix(&normalized) {
        return Err(ArchiveError::UnsafeArchivePath {
            archive_path: archive_entry_path.into(),
        });
    }

    let mut candidate = PathBuf::from(target_dir);
    for component in Path::new(&normalized).components() {
        match component {
            Component::Normal(value) => candidate.push(value),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(ArchiveError::UnsafeArchivePath {
                    archive_path: archive_entry_path.into(),
                });
            }
        }
    }

    if !path_starts_with(candidate.as_path(), target_dir) {
        return Err(ArchiveError::UnsafeArchivePath {
            archive_path: archive_entry_path.into(),
        });
    }

    Ok(candidate)
}

fn has_windows_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic()
}

fn path_starts_with(candidate: &Path, target_dir: &Path) -> bool {
    let target_components = normalized_components(target_dir);
    let candidate_components = normalized_components(candidate);
    candidate_components.starts_with(&target_components)
}

fn normalized_components(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().into_owned()),
            Component::Prefix(prefix) => Some(prefix.as_os_str().to_string_lossy().into_owned()),
            Component::RootDir => Some(String::from(std::path::MAIN_SEPARATOR)),
            Component::CurDir | Component::ParentDir => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::safe_join_extract_path;

    #[test]
    fn accepts_normal_relative_paths() {
        let path =
            safe_join_extract_path(std::path::Path::new("/tmp/out"), "dir/file.txt").unwrap();
        assert_eq!(
            path,
            std::path::Path::new("/tmp/out")
                .join("dir")
                .join("file.txt")
        );
    }

    #[test]
    fn blocks_parent_traversal() {
        assert!(safe_join_extract_path(std::path::Path::new("/tmp/out"), "../evil.exe").is_err());
    }

    #[test]
    fn blocks_absolute_unix_paths() {
        assert!(safe_join_extract_path(std::path::Path::new("/tmp/out"), "/etc/passwd").is_err());
    }

    #[test]
    fn blocks_windows_drive_paths() {
        assert!(safe_join_extract_path(
            std::path::Path::new("C:\\target"),
            "C:\\Windows\\system32\\cmd.exe"
        )
        .is_err());
    }

    #[test]
    fn blocks_unc_paths() {
        assert!(safe_join_extract_path(
            std::path::Path::new("C:\\target"),
            "\\\\server\\share\\evil.dll"
        )
        .is_err());
    }

    #[test]
    fn blocks_nested_parent_traversal() {
        assert!(
            safe_join_extract_path(std::path::Path::new("/tmp/out"), "folder/../../evil.exe")
                .is_err()
        );
    }

    #[test]
    fn accepts_plain_file_name() {
        let path = safe_join_extract_path(std::path::Path::new("/tmp/out"), "normal.txt").unwrap();
        assert_eq!(path, std::path::Path::new("/tmp/out").join("normal.txt"));
    }
}
