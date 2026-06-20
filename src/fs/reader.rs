use std::{fs, path::Path, time::SystemTime};

use anyhow::{Context, Result};

use crate::domain::entry::{Entry, EntryKind};

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
        let modified_at = metadata.modified().ok();

        entries.push(Entry {
            name: file_name.clone(),
            archive_path: None,
            remote_path: None,
            kind: if metadata.is_dir() {
                EntryKind::Directory
            } else {
                EntryKind::File
            },
            is_dir: metadata.is_dir(),
            size_bytes: metadata.len(),
            modified_at,
            attributes: format_attributes(&metadata, &file_name),
            is_parent_link: false,
        });
    }

    if path.parent().is_some() {
        entries.insert(0, Entry::parent_link());
    }

    Ok(entries)
}

pub fn read_entry(path: &Path, show_hidden_files: bool) -> Result<Option<Entry>> {
    let metadata = match fs::metadata(path) {
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
    let modified_at = metadata.modified().ok();

    Ok(Some(Entry {
        name: file_name.clone(),
        archive_path: None,
        remote_path: None,
        kind: if metadata.is_dir() {
            EntryKind::Directory
        } else {
            EntryKind::File
        },
        is_dir: metadata.is_dir(),
        size_bytes: metadata.len(),
        modified_at,
        attributes: format_attributes(&metadata, &file_name),
        is_parent_link: false,
    }))
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

pub fn format_system_time(timestamp: SystemTime) -> String {
    let datetime: chrono::DateTime<chrono::Local> = timestamp.into();
    datetime.format("%Y-%m-%d %H:%M").to_string()
}

#[cfg(target_os = "windows")]
fn format_attributes(metadata: &fs::Metadata, name: &str) -> String {
    let mut flags = Vec::new();

    if metadata.permissions().readonly() {
        flags.push("R");
    }
    if metadata.is_dir() {
        flags.push("D");
    }
    if name.starts_with('.') {
        flags.push("H");
    }

    #[cfg(target_os = "windows")]
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
    }

    if flags.is_empty() {
        "-".into()
    } else {
        flags.join("")
    }
}

#[cfg(not(target_os = "windows"))]
fn format_attributes(metadata: &fs::Metadata, _name: &str) -> String {
    format_unix_attributes(metadata)
}

#[cfg(not(target_os = "windows"))]
fn format_unix_attributes(metadata: &fs::Metadata) -> String {
    use std::os::unix::fs::{FileTypeExt, PermissionsExt};

    let file_type = metadata.file_type();
    let kind = if file_type.is_dir() {
        'd'
    } else if file_type.is_symlink() {
        'l'
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

fn is_hidden(metadata: &fs::Metadata, name: &str) -> bool {
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
mod tests {
    use super::{read_entries, read_entry};
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_dir_path(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("rcommander_{name}_{unique}"))
    }

    #[test]
    fn hides_dotfiles_when_disabled() {
        let path = temp_dir_path("hidden_listing");
        fs::create_dir(&path).unwrap();
        fs::write(path.join("visible.txt"), b"visible").unwrap();
        fs::write(path.join(".hidden.txt"), b"hidden").unwrap();

        let entries = read_entries(&path, false).unwrap();
        let _ = fs::remove_dir_all(&path);

        assert!(entries.iter().any(|entry| entry.name == "visible.txt"));
        assert!(!entries.iter().any(|entry| entry.name == ".hidden.txt"));
    }

    #[test]
    fn includes_dotfiles_when_enabled() {
        let path = temp_dir_path("show_hidden_listing");
        fs::create_dir(&path).unwrap();
        fs::write(path.join(".hidden.txt"), b"hidden").unwrap();

        let entries = read_entries(&path, true).unwrap();
        let _ = fs::remove_dir_all(&path);

        assert!(entries.iter().any(|entry| entry.name == ".hidden.txt"));
    }

    #[test]
    fn read_entry_returns_none_for_missing_path() {
        let path = temp_dir_path("missing_entry");
        let entry = read_entry(&path.join("missing.txt"), true).unwrap();
        assert!(entry.is_none());
    }
}
