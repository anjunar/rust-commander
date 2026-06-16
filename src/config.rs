use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const APP_DIR_NAME: &str = "rust-commander";
const CONFIG_FILE_NAME: &str = "config.toml";

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct AppConfig {
    #[serde(default)]
    pub window: WindowConfig,
    #[serde(default)]
    pub archive: ArchiveConfig,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct WindowConfig {
    pub width: i32,
    pub height: i32,
    pub position: Option<WindowPosition>,
    pub maximized: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WindowPosition {
    pub x: i32,
    pub y: i32,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            width: 1180,
            height: 760,
            position: Some(WindowPosition { x: 0, y: 0 }),
            maximized: false,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ArchiveConfig {
    pub seven_zip_path: Option<PathBuf>,
}

pub fn load() -> Result<AppConfig> {
    let path = config_file_path().context("Could not determine config file path")?;
    load_from_path(&path)
}

pub fn save(config: &AppConfig) -> Result<()> {
    let path = config_file_path().context("Could not determine config file path")?;
    save_to_path(config, &path)
}

fn load_from_path(path: &Path) -> Result<AppConfig> {
    if !path.exists() {
        return Ok(AppConfig::default());
    }

    let raw = fs::read_to_string(path)
        .with_context(|| format!("Could not read config file {}", path.display()))?;
    let config = toml::from_str::<AppConfig>(&raw)
        .with_context(|| format!("Could not parse config file {}", path.display()))?;
    Ok(config)
}

fn save_to_path(config: &AppConfig, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Could not create config directory {}", parent.display()))?;
    }

    let raw = toml::to_string_pretty(config).context("Could not serialize app config")?;
    fs::write(path, raw).with_context(|| format!("Could not write config file {}", path.display()))
}

fn config_file_path() -> Option<PathBuf> {
    config_base_dir().map(|dir| dir.join(APP_DIR_NAME).join(CONFIG_FILE_NAME))
}

fn config_base_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(PathBuf::from)
    }

    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join("Library").join("Application Support"))
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", unix)))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{AppConfig, ArchiveConfig, WindowConfig, WindowPosition, load_from_path, save_to_path};

    #[test]
    fn config_roundtrip_preserves_window_state() {
        let temp_path = std::env::temp_dir().join(format!(
            "rust_commander_config_test_{}.toml",
            std::process::id()
        ));
        let config = AppConfig {
            window: WindowConfig {
                width: 1440,
                height: 900,
                position: Some(WindowPosition { x: 120, y: 80 }),
                maximized: true,
            },
            archive: ArchiveConfig::default(),
        };

        save_to_path(&config, &temp_path).unwrap();
        let loaded = load_from_path(&temp_path).unwrap();
        let _ = std::fs::remove_file(&temp_path);

        assert_eq!(loaded.window.width, 1440);
        assert_eq!(loaded.window.height, 900);
        assert_eq!(loaded.window.position.as_ref().unwrap().x, 120);
        assert_eq!(loaded.window.position.as_ref().unwrap().y, 80);
        assert!(loaded.window.maximized);
    }
}
