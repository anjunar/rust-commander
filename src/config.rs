use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::remote::RemoteConfig;

const APP_DIR_NAME: &str = "rust-commander";
const CONFIG_FILE_NAME: &str = "config.toml";

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct AppConfig {
    #[serde(default)]
    pub window: WindowConfig,
    #[serde(default)]
    pub panes: PaneConfig,
    #[serde(default)]
    pub archive: ArchiveConfig,
    #[serde(default)]
    pub locale: LocaleConfig,
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub panels: PanelSettings,
    #[serde(default)]
    pub file_operations: FileOperationsConfig,
    #[serde(default)]
    pub viewer: ViewerConfig,
    #[serde(default)]
    pub remote: RemoteConfig,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct PaneConfig {
    pub left_directory: Option<PathBuf>,
    pub right_directory: Option<PathBuf>,
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
pub struct ArchiveConfig {}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct LocaleConfig {
    pub language: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct GeneralConfig {
    pub theme: ThemePreference,
    pub restore_last_session: bool,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            theme: ThemePreference::System,
            restore_last_session: true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct PanelSettings {
    pub show_hidden_files: bool,
    pub folders_first: bool,
    pub left_start_path: Option<PathBuf>,
    pub right_start_path: Option<PathBuf>,
}

impl Default for PanelSettings {
    fn default() -> Self {
        Self {
            show_hidden_files: false,
            folders_first: true,
            left_start_path: None,
            right_start_path: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct FileOperationsConfig {
    pub use_recycle_bin: bool,
    pub confirm_delete: bool,
    pub confirm_overwrite: bool,
}

impl Default for FileOperationsConfig {
    fn default() -> Self {
        Self {
            use_recycle_bin: true,
            confirm_delete: true,
            confirm_overwrite: true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct ViewerConfig {
    pub streaming_threshold_mb: u64,
    pub line_wrap: bool,
    pub show_line_numbers: bool,
}

impl ViewerConfig {
    pub fn streaming_threshold_bytes(&self) -> u64 {
        self.streaming_threshold_mb
            .saturating_mul(1024)
            .saturating_mul(1024)
    }
}

impl Default for ViewerConfig {
    fn default() -> Self {
        Self {
            streaming_threshold_mb: 20,
            line_wrap: false,
            show_line_numbers: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

pub fn load_or_default() -> AppConfig {
    let Some(path) = config_file_path() else {
        eprintln!("Could not determine config file path");
        return AppConfig::default();
    };

    load_or_default_from_path(&path)
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

fn load_or_default_from_path(path: &Path) -> AppConfig {
    match load_from_path(path) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("Could not load config {}: {error}", path.display());
            AppConfig::default()
        }
    }
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
#[path = "../tests/unit/config_tests.rs"]
mod tests;
