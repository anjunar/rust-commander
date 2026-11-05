use std::path::PathBuf;

use super::{
    load_from_path, load_or_default_from_path, save_to_path, AppConfig, ArchiveConfig,
    EditorConfig, FavoriteEntry, FileOperationsConfig, GeneralConfig, IconStyle, LocaleConfig,
    PaneConfig, PanelSettings, ThemePreference, ViewerConfig, WindowConfig, WindowPosition,
};
use crate::remote::RemoteConfig;

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
        panes: PaneConfig {
            left_tabs: Vec::new(),
            right_tabs: Vec::new(),
            left_active_tab: 0,
            right_active_tab: 0,
            left_directory: Some(PathBuf::from("/tmp/left")),
            right_directory: Some(PathBuf::from("/tmp/right")),
            left_cursor: Some("alpha.txt".into()),
            right_cursor: Some("..".into()),
        },
        archive: ArchiveConfig::default(),
        locale: LocaleConfig {
            language: Some("fr".into()),
        },
        general: GeneralConfig {
            theme: ThemePreference::Dark,
            icon_style: IconStyle::System,
            restore_last_session: false,
        },
        panels: PanelSettings {
            show_hidden_files: true,
            folders_first: false,
            left_start_path: Some(PathBuf::from("/tmp/start-left")),
            right_start_path: Some(PathBuf::from("/tmp/start-right")),
        },
        file_operations: FileOperationsConfig {
            use_recycle_bin: false,
            confirm_delete: false,
            confirm_overwrite: false,
        },
        viewer: ViewerConfig {
            streaming_threshold_mb: 64,
            line_wrap: true,
            show_line_numbers: false,
        },
        editor: EditorConfig {
            width: 1100,
            height: 800,
            maximized: true,
            max_edit_size_mb: 15,
        },
        remote: RemoteConfig::default(),
        favorites: vec![FavoriteEntry {
            name: "Temp".into(),
            path: PathBuf::from("/tmp"),
        }],
        folder_sort: Vec::new(),
    };

    save_to_path(&config, &temp_path).unwrap();
    let loaded = load_from_path(&temp_path).unwrap();
    let _ = std::fs::remove_file(&temp_path);

    assert_eq!(loaded.window.width, 1440);
    assert_eq!(loaded.window.height, 900);
    assert_eq!(loaded.window.position.as_ref().unwrap().x, 120);
    assert_eq!(loaded.window.position.as_ref().unwrap().y, 80);
    assert!(loaded.window.maximized);
    assert_eq!(
        loaded.panes.left_directory,
        Some(PathBuf::from("/tmp/left"))
    );
    assert_eq!(
        loaded.panes.right_directory,
        Some(PathBuf::from("/tmp/right"))
    );
    assert_eq!(loaded.panes.left_cursor.as_deref(), Some("alpha.txt"));
    assert_eq!(loaded.panes.right_cursor.as_deref(), Some(".."));
    assert_eq!(loaded.locale.language.as_deref(), Some("fr"));
    assert_eq!(loaded.general.theme, ThemePreference::Dark);
    assert_eq!(loaded.general.icon_style, IconStyle::System);
    assert!(!loaded.general.restore_last_session);
    assert!(loaded.panels.show_hidden_files);
    assert!(!loaded.panels.folders_first);
    assert_eq!(
        loaded.panels.left_start_path,
        Some(PathBuf::from("/tmp/start-left"))
    );
    assert_eq!(
        loaded.panels.right_start_path,
        Some(PathBuf::from("/tmp/start-right"))
    );
    assert!(!loaded.file_operations.use_recycle_bin);
    assert!(!loaded.file_operations.confirm_delete);
    assert!(!loaded.file_operations.confirm_overwrite);
    assert_eq!(loaded.viewer.streaming_threshold_mb, 64);
    assert!(loaded.viewer.line_wrap);
    assert!(!loaded.viewer.show_line_numbers);
    assert_eq!(loaded.editor.width, 1100);
    assert_eq!(loaded.editor.height, 800);
    assert!(loaded.editor.maximized);
    assert_eq!(loaded.editor.max_edit_size_mb, 15);
    assert_eq!(
        loaded.favorites,
        vec![FavoriteEntry {
            name: "Temp".into(),
            path: PathBuf::from("/tmp"),
        }]
    );
}

#[test]
fn old_config_without_new_settings_uses_defaults() {
    let temp_path = std::env::temp_dir().join(format!(
        "rust_commander_old_config_test_{}.toml",
        std::process::id()
    ));
    let raw = r#"
[window]
width = 1280
height = 720
maximized = false

[panes]
left_directory = "/tmp/left"
right_directory = "/tmp/right"

[locale]
language = "de"
"#;
    std::fs::write(&temp_path, raw).unwrap();

    let loaded = load_from_path(&temp_path).unwrap();
    let _ = std::fs::remove_file(&temp_path);

    assert_eq!(loaded.window.width, 1280);
    assert_eq!(loaded.locale.language.as_deref(), Some("de"));
    assert_eq!(loaded.general.theme, ThemePreference::System);
    assert!(loaded.general.restore_last_session);
    assert!(loaded.panes.left_cursor.is_none());
    assert!(loaded.panes.right_cursor.is_none());
    assert!(!loaded.panels.show_hidden_files);
    assert!(loaded.panels.folders_first);
    assert!(loaded.file_operations.use_recycle_bin);
    assert!(loaded.file_operations.confirm_delete);
    assert!(loaded.file_operations.confirm_overwrite);
    assert_eq!(loaded.viewer.streaming_threshold_mb, 20);
    assert!(!loaded.viewer.line_wrap);
    assert!(loaded.viewer.show_line_numbers);
    assert_eq!(loaded.editor.width, EditorConfig::default().width);
    assert_eq!(loaded.editor.height, EditorConfig::default().height);
    assert!(!loaded.editor.maximized);
    assert!(loaded.favorites.is_empty());
}

#[test]
fn invalid_config_returns_defaults_in_load_or_default() {
    let temp_path = std::env::temp_dir().join(format!(
        "rust_commander_invalid_config_test_{}.toml",
        std::process::id()
    ));
    std::fs::write(&temp_path, "[window\nwidth = 800").unwrap();

    let loaded = load_or_default_from_path(&temp_path);
    let _ = std::fs::remove_file(&temp_path);

    assert_eq!(loaded.window.width, WindowConfig::default().width);
    assert_eq!(loaded.general.theme, ThemePreference::System);
}
