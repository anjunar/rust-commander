use std::{fs, path::Path, time::SystemTime};

use anyhow::{Context, Result};

use crate::{domain::entry::Entry, presentation};

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
            is_dir: metadata.is_dir(),
            size_bytes: metadata.len(),
            size_label: format_size(&metadata, metadata.is_dir()),
            type_label: presentation::filesystem_entry_type_label(metadata.is_dir()),
            modified_at,
            modified_label: format_modified(modified_at),
            attributes_label: format_attributes(&metadata, &file_name),
            is_parent_link: false,
        });
    }

    if path.parent().is_some() {
        entries.insert(
            0,
            Entry::parent_link(presentation::parent_entry_type_label()),
        );
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
    let Some(file_name) = path.file_name().map(|value| value.to_string_lossy().into_owned()) else {
        return Ok(None);
    };
    if !show_hidden_files && is_hidden(&metadata, &file_name) {
        return Ok(None);
    }
    let modified_at = metadata.modified().ok();

    Ok(Some(Entry {
        name: file_name.clone(),
        archive_path: None,
        is_dir: metadata.is_dir(),
        size_bytes: metadata.len(),
        size_label: format_size(&metadata, metadata.is_dir()),
        type_label: presentation::filesystem_entry_type_label(metadata.is_dir()),
        modified_at,
        modified_label: format_modified(modified_at),
        attributes_label: format_attributes(&metadata, &file_name),
        is_parent_link: false,
    }))
}

pub fn rename_path(source: &Path, target: &Path) -> Result<()> {
    if target.exists() {
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

fn format_size(metadata: &fs::Metadata, is_dir: bool) -> String {
    if is_dir {
        return "-".into();
    }

    format_bytes(metadata.len())
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

fn format_modified(modified_at: Option<SystemTime>) -> String {
    modified_at
        .map(|timestamp| {
            let datetime: chrono::DateTime<chrono::Local> = timestamp.into();
            datetime.format("%Y-%m-%d %H:%M").to_string()
        })
        .unwrap_or_else(|| "-".into())
}

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
