use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{bail, Context, Result};

use crate::domain::roots::RootLocation;
use crate::platform::context_menu::ContextMenuRequest;

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

pub fn open_console(path: &Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(["-a", "Terminal"])
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("Could not open a console for {}", path.display()))?;

        return Ok(());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        for (program, args) in [
            ("x-terminal-emulator", vec!["--working-directory"]),
            ("gnome-terminal", vec!["--working-directory"]),
            ("xfce4-terminal", vec!["--working-directory"]),
            ("konsole", vec!["--workdir"]),
            ("kitty", vec!["--directory"]),
            ("alacritty", vec!["--working-directory"]),
        ] {
            let result = Command::new(program)
                .args(&args)
                .arg(path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();

            match result {
                Ok(_) => return Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("Could not open a console for {}", path.display())
                    });
                }
            }
        }
    }

    anyhow::bail!(
        "No supported terminal application was found for {}",
        path.display()
    )
}

pub fn show_context_menu(_request: &ContextMenuRequest) -> Result<()> {
    anyhow::bail!("Native context menus are not implemented for this platform yet")
}

pub fn chmod_paths(paths: &[PathBuf], mode: &str, recursive: bool) -> Result<()> {
    if paths.is_empty() {
        bail!("No filesystem entries were selected");
    }

    let mode = mode.trim();
    if mode.is_empty() {
        bail!("The chmod mode must not be empty");
    }

    let mut command = Command::new("chmod");
    if recursive {
        command.arg("-R");
    }
    command.arg(mode);
    command.args(paths);

    run_privileged_command(command, "chmod")
}

pub fn chown_paths(paths: &[PathBuf], owner_spec: &str, recursive: bool) -> Result<()> {
    if paths.is_empty() {
        bail!("No filesystem entries were selected");
    }

    let owner_spec = owner_spec.trim();
    if owner_spec.is_empty() {
        bail!("The owner specification must not be empty");
    }

    let mut command = Command::new("chown");
    if recursive {
        command.arg("-R");
    }
    command.arg(owner_spec);
    command.args(paths);

    run_privileged_command(command, "chown")
}

fn run_privileged_command(mut command: Command, program: &str) -> Result<()> {
    let output = command
        .output()
        .with_context(|| format!("Could not start {program}"))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        bail!("{program} failed with exit code {:?}", output.status.code());
    }

    bail!("{program} failed: {stderr}")
}
