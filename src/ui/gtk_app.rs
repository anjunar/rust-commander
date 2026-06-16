use anyhow::Result;
use gtk::prelude::*;

use crate::{application::Commander, config, i18n, ui::main_window::MainWindow};

pub const APP_ID: &str = "dev.rcommander.Gtk";

pub fn run() -> Result<()> {
    let app = gtk::Application::builder().application_id(APP_ID).build();

    #[cfg(target_os = "windows")]
    app.connect_shutdown(|_| {
        crate::platform::tray::remove_tray_icon();
    });

    app.connect_activate(|app| {
        let app_config = config::load_or_default();
        i18n::apply_locale(app_config.locale.language.as_deref());

        let fallback_path = match std::env::current_dir() {
            Ok(path) => path,
            Err(error) => {
                eprintln!("Could not determine current directory: {error}");
                return;
            }
        };

        let left_initial_path = initial_panel_path(
            app_config.general.restore_last_session,
            app_config.panes.left_directory.clone(),
            app_config.panels.left_start_path.clone(),
            &fallback_path,
        );
        let right_initial_path = initial_panel_path(
            app_config.general.restore_last_session,
            app_config.panes.right_directory.clone(),
            app_config.panels.right_start_path.clone(),
            &fallback_path,
        );

        let commander = match Commander::new(
            left_initial_path,
            right_initial_path,
            app_config.archive.clone(),
            app_config.panels.clone(),
        ) {
            Ok(commander) => commander,
            Err(error) => {
                eprintln!("Could not initialize RCommander: {error}");
                return;
            }
        };

        let _window = MainWindow::new(app, commander, app_config);
    });

    app.run();
    Ok(())
}

fn initial_panel_path(
    restore_last_session: bool,
    last_session_path: Option<std::path::PathBuf>,
    configured_start_path: Option<std::path::PathBuf>,
    fallback_path: &std::path::Path,
) -> std::path::PathBuf {
    let preferred = if restore_last_session {
        last_session_path.or(configured_start_path)
    } else {
        configured_start_path
    };

    preferred
        .clone()
        .filter(|path| path.is_dir())
        .unwrap_or_else(|| fallback_path.to_path_buf())
}
