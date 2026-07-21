use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::domain::entry::EntryKind;

/// Classifies a filesystem entry using link-aware metadata.
///
/// Symlinks keep `EntryKind::Symlink` but set `is_dir` from the *target*
/// so directory links sort/navigate like folders. Windows `.lnk` shortcuts
/// to directories are treated the same way.
pub fn classify_fs_entry(path: &Path, metadata: &fs::Metadata) -> (EntryKind, bool, u64) {
    if metadata.file_type().is_symlink() {
        let target_is_dir = fs::metadata(path)
            .map(|meta| meta.is_dir())
            .unwrap_or_else(|_| directory_bit_set(metadata));
        return (
            EntryKind::Symlink,
            target_is_dir,
            if target_is_dir { 0 } else { metadata.len() },
        );
    }

    #[cfg(target_os = "windows")]
    if is_windows_shortcut(path) {
        if let Some(target) = resolve_windows_shortcut_target(path) {
            let target_is_dir = target.is_dir();
            return (
                EntryKind::Symlink,
                target_is_dir,
                if target_is_dir { 0 } else { metadata.len() },
            );
        }
        return (EntryKind::Symlink, false, metadata.len());
    }

    if metadata.is_dir() {
        (EntryKind::Directory, true, metadata.len())
    } else {
        (EntryKind::File, false, metadata.len())
    }
}

/// Resolves the path that should be opened when navigating into a directory-like entry.
///
/// Prefer the original path when it is listable (normal symlink/junction UX).
/// If listing fails — common for restricted Windows junctions like
/// `C:\Documents and Settings` — fall back to the resolved target when that is
/// readable. Windows folder shortcuts always resolve to their target.
pub fn resolve_directory_navigation_path(path: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    if is_windows_shortcut(path) {
        if let Some(target) = resolve_windows_shortcut_target(path) {
            if target.is_dir() {
                return target;
            }
        }
    }

    if fs::read_dir(path).is_ok() {
        return path.to_path_buf();
    }

    if let Some(target) = absolute_symlink_target(path) {
        if fs::read_dir(&target).is_ok() {
            return target;
        }
    }

    path.to_path_buf()
}

fn absolute_symlink_target(path: &Path) -> Option<PathBuf> {
    let meta = fs::symlink_metadata(path).ok()?;
    if !meta.file_type().is_symlink() {
        return None;
    }
    let target = fs::read_link(path).ok()?;
    let absolute = if target.is_absolute() {
        target
    } else {
        path.parent()?.join(target)
    };
    Some(normalize_os_path(absolute))
}

fn normalize_os_path(path: PathBuf) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let lossy = path.to_string_lossy();
        for prefix in [r"\\?\UNC\", r"\\?\", r"\??\"] {
            if let Some(stripped) = lossy.strip_prefix(prefix) {
                if prefix == r"\\?\UNC\" {
                    return PathBuf::from(format!(r"\\{stripped}"));
                }
                return PathBuf::from(stripped);
            }
        }
    }
    path
}

fn directory_bit_set(metadata: &fs::Metadata) -> bool {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x10 != 0
    }
    #[cfg(not(target_os = "windows"))]
    {
        metadata.is_dir()
    }
}

#[cfg(target_os = "windows")]
fn is_windows_shortcut(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lnk"))
}

#[cfg(target_os = "windows")]
fn resolve_windows_shortcut_target(path: &Path) -> Option<PathBuf> {
    crate::platform::windows::resolve_shortcut_target(path)
}
