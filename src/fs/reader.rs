use std::{fs, path::Path, time::SystemTime};

use anyhow::{Context, Result};

use crate::domain::entry::Entry;

pub fn read_entries(path: &Path) -> Result<Vec<Entry>> {
    let mut entries = fs::read_dir(path)
        .with_context(|| format!("Could not read directory {}", path.display()))?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let metadata = entry.metadata().ok()?;
            let file_name = entry.file_name().to_string_lossy().into_owned();
            let modified_at = metadata.modified().ok();

            Some(Entry {
                name: file_name.clone(),
                archive_path: None,
                is_dir: metadata.is_dir(),
                size_bytes: metadata.len(),
                size_label: format_size(&metadata, metadata.is_dir()),
                type_label: if metadata.is_dir() { "Folder" } else { "File" }.into(),
                modified_at,
                modified_label: format_modified(modified_at),
                attributes_label: format_attributes(&metadata, &file_name),
                is_parent_link: false,
            })
        })
        .collect::<Vec<_>>();

    if path.parent().is_some() {
        entries.insert(0, Entry::parent_link());
    }

    Ok(entries)
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
