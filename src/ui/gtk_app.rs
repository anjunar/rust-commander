use anyhow::Result;
use gtk::prelude::*;

use crate::{application::Commander, config, ui::main_window::MainWindow};

pub fn run() -> Result<()> {
    let app = gtk::Application::builder()
        .application_id("dev.rcommander.Gtk")
        .build();

    #[cfg(target_os = "windows")]
    app.connect_shutdown(|_| {
        crate::platform::tray::remove_tray_icon();
    });

    app.connect_activate(|app| {
        let app_config = match config::load() {
            Ok(config) => config,
            Err(error) => {
                eprintln!("Could not load config: {error}");
                config::AppConfig::default()
            }
        };

        let initial_path = match std::env::current_dir() {
            Ok(path) => path,
            Err(error) => {
                eprintln!("Could not determine current directory: {error}");
                return;
            }
        };

        let commander = match Commander::new(initial_path, app_config.archive.clone()) {
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
