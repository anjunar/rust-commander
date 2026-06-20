use std::rc::Rc;

use anyhow::Result;
use gtk::prelude::*;

use crate::{
    application::{system_platform_port, Commander},
    config, i18n, presentation,
    ui::main_window::{MainWindow, MainWindowRuntime},
};

pub const APP_ID: &str = "dev.rcommander.Gtk";

pub fn run() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        configure_macos_runtime();
    }

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
        let platform_port = system_platform_port();

        let commander = Commander::new(
            left_initial_path,
            right_initial_path,
            app_config.panels.clone(),
            platform_port.available_roots(),
            presentation::ready_status(),
        );

        let runtime = MainWindowRuntime::new(commander, app_config, platform_port);
        let window = MainWindow::new_hidden(app, runtime);
        window.on_initial_panels_ready(Rc::new({
            let window = Rc::clone(&window);
            move || {
                window.present_window();
            }
        }));
    });

    app.run();
    Ok(())
}

#[cfg(target_os = "macos")]
fn configure_macos_runtime() {
    let Ok(exe_path) = std::env::current_exe() else {
        return;
    };
    let Some(macos_dir) = exe_path.parent() else {
        return;
    };
    let Some(contents_dir) = macos_dir.parent() else {
        return;
    };

    let resources_dir = contents_dir.join("Resources");
    let frameworks_dir = contents_dir.join("Frameworks");
    let resources_bin_dir = resources_dir.join("bin");
    let resources_share_dir = resources_dir.join("share");
    let gtk_lib_dir = resources_dir.join("lib").join("gtk-4.0");
    let glib_schema_dir = resources_share_dir.join("glib-2.0").join("schemas");

    prepend_env_path("PATH", &resources_bin_dir);
    prepend_env_path("XDG_DATA_DIRS", &resources_share_dir);

    if glib_schema_dir.is_dir() {
        std::env::set_var("GSETTINGS_SCHEMA_DIR", &glib_schema_dir);
    }
    if resources_dir.is_dir() {
        std::env::set_var("GTK_DATA_PREFIX", &resources_dir);
        std::env::set_var("GTK_EXE_PREFIX", &resources_dir);
    }
    if gtk_lib_dir.is_dir() {
        std::env::set_var("GTK_PATH", &gtk_lib_dir);
    }
    if frameworks_dir.is_dir() {
        prepend_env_path("DYLD_FALLBACK_LIBRARY_PATH", &frameworks_dir);
    }
    if std::env::var_os("GDK_BACKEND").is_none() {
        std::env::set_var("GDK_BACKEND", "macos");
    }
    if std::env::var_os("GSK_RENDERER").is_none() {
        std::env::set_var("GSK_RENDERER", "cairo");
    }

    if let Some(loaders_dir) =
        find_first_named_dir(&resources_dir.join("lib").join("gdk-pixbuf-2.0"), "loaders")
    {
        std::env::set_var("GDK_PIXBUF_MODULEDIR", &loaders_dir);
    }
    let pixbuf_cache = resources_dir
        .join("lib")
        .join("gdk-pixbuf-2.0")
        .join("2.10.0")
        .join("loaders.cache");
    if pixbuf_cache.is_file() {
        std::env::set_var("GDK_PIXBUF_MODULE_FILE", &pixbuf_cache);
    }

    let immodules_cache = gtk_lib_dir.join("gtk.immodules");
    if immodules_cache.is_file() {
        std::env::set_var("GTK_IM_MODULE_FILE", &immodules_cache);
    }
}

#[cfg(target_os = "macos")]
fn prepend_env_path(key: &str, value: &std::path::Path) {
    if !value.exists() {
        return;
    }

    match std::env::var_os(key) {
        Some(existing) if !existing.is_empty() => {
            let mut combined = std::path::PathBuf::from(value).into_os_string();
            combined.push(":");
            combined.push(existing);
            std::env::set_var(key, combined);
        }
        _ => {
            std::env::set_var(key, value);
        }
    }
}

#[cfg(target_os = "macos")]
fn find_first_named_dir(root: &std::path::Path, target_name: &str) -> Option<std::path::PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|name| name.to_str()) == Some(target_name) {
                return Some(path);
            }
            if let Some(found) = find_first_named_dir(&path, target_name) {
                return Some(found);
            }
        }
    }
    None
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
