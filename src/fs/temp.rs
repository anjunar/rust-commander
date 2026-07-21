use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};

/// Create a unique temp directory for a remote edit session.
pub fn create_remote_edit_temp_dir(file_name: &str) -> Result<(PathBuf, PathBuf)> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let dir = std::env::temp_dir()
        .join("rust-commander")
        .join("edit")
        .join(format!("{stamp}-{}", std::process::id()));
    fs::create_dir_all(&dir)
        .with_context(|| format!("Could not create temp edit directory {}", dir.display()))?;

    let safe_name = sanitize_file_name(file_name);
    let file_path = dir.join(&safe_name);
    Ok((dir, file_path))
}

pub fn remove_temp_dir(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

fn sanitize_file_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return "remote-file.txt".into();
    }
    trimmed
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            _ => ch,
        })
        .collect()
}
