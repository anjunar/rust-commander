use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result};

use crate::domain::roots::RootLocation;

pub fn available_roots() -> Vec<RootLocation> {
    let mut roots = Vec::new();

    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        roots.push(RootLocation {
            label: "Home".into(),
            path: home,
        });
    }

    let root_path = PathBuf::from("/");
    if !roots.iter().any(|root| root.path == root_path) {
        roots.push(RootLocation {
            label: "/".into(),
            path: root_path,
        });
    }

    for base in ["/Volumes", "/media", "/mnt"] {
        let base_path = Path::new(base);
        if let Ok(entries) = std::fs::read_dir(base_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && !roots.iter().any(|root| root.path == path) {
                    let label = path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.display().to_string());
                    roots.push(RootLocation { label, path });
                }
            }
        }
    }

    roots
}

pub fn open_path(path: &Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    let opener = "open";

    #[cfg(all(unix, not(target_os = "macos")))]
    let opener = "xdg-open";

    #[cfg(not(unix))]
    let opener = "";

    Command::new(opener)
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("Could not launch the default action for {}", path.display()))?;

    Ok(())
}
