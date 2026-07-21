use std::{fs, path::Path, time::SystemTime};

use anyhow::{Context, Result};

use crate::domain::entry::{Entry, EntryKind};

use super::links::classify_fs_entry;

pub use super::links::resolve_directory_navigation_path;

pub fn read_entries(path: &Path, show_hidden_files: bool) -> Result<Vec<Entry>> {
    let mut entries = Vec::new();

    for entry in fs::read_dir(path)
        .with_context(|| format!("Could not read directory {}", path.display()))?
    {
        let entry =
            entry.with_context(|| format!("Could not read an entry in {}", path.display()))?;
        let entry_path = entry.path();
        let metadata = entry
            .metadata()
            .with_context(|| format!("Could not read metadata for {}", entry_path.display()))?;
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if !show_hidden_files && is_hidden(&metadata, &file_name) {
            continue;
        }
        entries.push(entry_from_metadata(&entry_path, file_name, &metadata));
    }

    if path.parent().is_some() {
        entries.insert(0, Entry::parent_link());
    }

    Ok(entries)
}

pub fn read_entry(path: &Path, show_hidden_files: bool) -> Result<Option<Entry>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("Could not read metadata for {}", path.display()));
        }
    };
    let Some(file_name) = path
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
    else {
        return Ok(None);
    };
    if !show_hidden_files && is_hidden(&metadata, &file_name) {
        return Ok(None);
    }

    Ok(Some(entry_from_metadata(path, file_name, &metadata)))
}

fn entry_from_metadata(path: &Path, file_name: String, metadata: &fs::Metadata) -> Entry {
    let (kind, is_dir, size_bytes) = classify_fs_entry(path, metadata);
    let modified_at = metadata.modified().ok();

    Entry {
        name: file_name.clone(),
        archive_path: None,
        remote_path: None,
        kind,
        is_dir,
        size_bytes,
        modified_at,
        attributes: format_attributes(metadata, &file_name, is_dir, kind),
        is_parent_link: false,
    }
}

pub fn rename_path(source: &Path, target: &Path) -> Result<()> {
    if target.exists() && !paths_resolve_to_same_entry(source, target) {
        anyhow::bail!("An entry with this name already exists");
    }

    fs::rename(source, target).with_context(|| {
        format!(
            "Could not rename {} to {}",
            source.display(),
            target.display()
        )
    })
}

fn paths_resolve_to_same_entry(source: &Path, target: &Path) -> bool {
    let Ok(source_path) = fs::canonicalize(source) else {
        return false;
    };
    let Ok(target_path) = fs::canonicalize(target) else {
        return false;
    };
    source_path == target_path
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut unit_index = 0usize;
    let mut value = bytes as f64;

    while value >= 1024.0 && unit_index + 1 < UNITS.len() {
        value /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{bytes} {}", UNITS[unit_index])
    } else {
        format!("{value:.1} {}", UNITS[unit_index])
    }
}

/// Recursively sums file sizes under `path` (directories themselves add 0).
/// Directory links are followed once at the root via OS path semantics; nested
/// symlink directories are not re-entered (non-following child metadata).
pub fn directory_total_size(path: &Path) -> u64 {
    let root = resolve_directory_navigation_path(path);
    let mut total = 0u64;
    let mut stack = vec![root];

    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.is_dir() {
                // Avoid descending into nested directory links/junctions.
                if metadata.file_type().is_symlink() {
                    continue;
                }
                stack.push(entry.path());
            } else {
                total = total.saturating_add(metadata.len());
            }
        }
    }

    total
}

pub fn format_system_time(timestamp: SystemTime) -> String {
    let datetime: chrono::DateTime<chrono::Local> = timestamp.into();
    datetime.format("%Y-%m-%d %H:%M").to_string()
}

#[cfg(target_os = "windows")]
fn format_attributes(
    metadata: &fs::Metadata,
    name: &str,
    is_dir: bool,
    kind: EntryKind,
) -> String {
    let mut flags = Vec::new();

    if metadata.permissions().readonly() {
        flags.push("R");
    }
    if is_dir {
        flags.push("D");
    }
    if matches!(kind, EntryKind::Symlink) || metadata.file_type().is_symlink() {
        flags.push("L");
    }
    if name.starts_with('.') {
        flags.push("H");
    }

    {
        use std::os::windows::fs::MetadataExt;

        let attributes = metadata.file_attributes();
        if attributes & 0x2 != 0 && !flags.contains(&"H") {
            flags.push("H");
        }
        if attributes & 0x20 != 0 {
            flags.push("A");
        }
        if attributes & 0x4 != 0 {
            flags.push("S");
        }
        if attributes & 0x400 != 0 && !flags.contains(&"L") {
            flags.push("L");
        }
    }

    if flags.is_empty() {
        "-".into()
    } else {
        flags.join("")
    }
}

#[cfg(not(target_os = "windows"))]
fn format_attributes(
    metadata: &fs::Metadata,
    _name: &str,
    _is_dir: bool,
    _kind: EntryKind,
) -> String {
    format_unix_attributes(metadata)
}

#[cfg(not(target_os = "windows"))]
fn format_unix_attributes(metadata: &fs::Metadata) -> String {
    use std::os::unix::fs::{FileTypeExt, PermissionsExt};

    let file_type = metadata.file_type();
    let kind = if file_type.is_symlink() {
        'l'
    } else if file_type.is_dir() {
        'd'
    } else if file_type.is_socket() {
        's'
    } else if file_type.is_fifo() {
        'p'
    } else if file_type.is_char_device() {
        'c'
    } else if file_type.is_block_device() {
        'b'
    } else {
        '-'
    };

    let mode = metadata.permissions().mode();
    let mut attributes = String::with_capacity(10);
    attributes.push(kind);
    attributes.push(permission_char(mode, 0o400, 'r'));
    attributes.push(permission_char(mode, 0o200, 'w'));
    attributes.push(execute_char(mode, 0o100, 0o4000, 's', 'S'));
    attributes.push(permission_char(mode, 0o040, 'r'));
    attributes.push(permission_char(mode, 0o020, 'w'));
    attributes.push(execute_char(mode, 0o010, 0o2000, 's', 'S'));
    attributes.push(permission_char(mode, 0o004, 'r'));
    attributes.push(permission_char(mode, 0o002, 'w'));
    attributes.push(execute_char(mode, 0o001, 0o1000, 't', 'T'));
    attributes
}

#[cfg(not(target_os = "windows"))]
fn permission_char(mode: u32, bit: u32, value: char) -> char {
    if mode & bit != 0 {
        value
    } else {
        '-'
    }
}

#[cfg(not(target_os = "windows"))]
fn execute_char(
    mode: u32,
    execute_bit: u32,
    special_bit: u32,
    set_char: char,
    unset_char: char,
) -> char {
    match (mode & execute_bit != 0, mode & special_bit != 0) {
        (true, true) => set_char,
        (false, true) => unset_char,
        (true, false) => 'x',
        (false, false) => '-',
    }
}

pub(crate) fn is_hidden(metadata: &fs::Metadata, name: &str) -> bool {
    if name == "." || name == ".." {
        return false;
    }

    if name.starts_with('.') {
        return true;
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x2 != 0
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = metadata;
        false
    }
}

#[cfg(test)]
#[path = "../../tests/unit/fs_reader_tests.rs"]
mod tests;
