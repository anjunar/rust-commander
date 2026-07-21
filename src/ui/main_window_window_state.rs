use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

use gtk::{glib, prelude::*};

#[cfg(target_os = "windows")]
use crate::platform::restore_window_placement;

use crate::{
    application::{ActivePanel, Commander, ConfigStore},
    config::{AppConfig, WindowConfig, WindowPosition},
    platform::current_window_placement,
};

use super::APP_WINDOW_TITLE;

#[derive(Clone)]
pub struct WindowStateController {
    window: gtk::ApplicationWindow,
    horizontal_paned: gtk::Paned,
    vertical_paned: gtk::Paned,
    commander: Rc<RefCell<Commander>>,
    config_store: ConfigStore,
}

impl WindowStateController {
    pub fn new(
        window: gtk::ApplicationWindow,
        horizontal_paned: gtk::Paned,
        vertical_paned: gtk::Paned,
        commander: Rc<RefCell<Commander>>,
        config_store: ConfigStore,
    ) -> Self {
        Self {
            window,
            horizontal_paned,
            vertical_paned,
            commander,
            config_store,
        }
    }

    pub fn install_window_state_persistence(&self) {
        let commander = Rc::clone(&self.commander);
        let config_store = self.config_store.clone();
        self.window.connect_close_request(move |window| {
            if let Err(error) = config_store.update(|app_config| {
                let commander = commander.borrow();
                app_config.window.maximized = window.is_maximized();
                if !app_config.window.maximized {
                    app_config.window.width = window.width().max(1);
                    app_config.window.height = window.height().max(1);
                }
                write_pane_session(app_config, &commander);
            }) {
                eprintln!("Could not save config: {error}");
            }

            // Editor windows keep the GTK application alive; close them with the commander.
            if close_auxiliary_windows(window) {
                return glib::Propagation::Stop;
            }

            glib::Propagation::Proceed
        });
    }

    /// Persist panel tabs (paths, cursors, sort) immediately — e.g. after sort changes.
    pub fn persist_pane_session(&self) {
        if let Err(error) = persist_pane_session(&self.commander.borrow(), &self.config_store) {
            eprintln!("Could not save pane session: {error}");
        }
    }

    pub fn install_window_geometry_tracking(&self) {
        let window = self.window.clone();
        let app_config_cache = self.config_store.cache();
        glib::timeout_add_local(Duration::from_millis(250), move || {
            let mut app_config = app_config_cache.borrow_mut();
            let config = &mut app_config.window;
            config.maximized = window.is_maximized();
            if let Some(placement) = current_window_placement(APP_WINDOW_TITLE) {
                config.width = placement.width.max(1);
                config.height = placement.height.max(1);
                config.position = Some(WindowPosition {
                    x: placement.x,
                    y: placement.y,
                });
                config.maximized = placement.maximized;
            } else if !config.maximized {
                config.width = window.width().max(1);
                config.height = window.height().max(1);
            }
            glib::ControlFlow::Continue
        });
    }

    pub fn restore_window_geometry(&self, window_config: WindowConfig) {
        #[cfg(not(target_os = "windows"))]
        {
            self.window
                .set_default_size(window_config.width.max(1), window_config.height.max(1));
            if window_config.maximized {
                let window = self.window.clone();
                glib::idle_add_local_once(move || {
                    window.maximize();
                });
            }
        }

        #[cfg(target_os = "windows")]
        {
            let position = window_config
                .position
                .unwrap_or(WindowPosition { x: 0, y: 0 });
            let width = window_config.width.max(1);
            let height = window_config.height.max(1);
            let maximized = window_config.maximized;
            let window = self.window.clone();

            glib::idle_add_local_once({
                let position = position.clone();
                let window = window.clone();
                move || {
                    restore_window_placement(
                        APP_WINDOW_TITLE,
                        position.x,
                        position.y,
                        width,
                        height,
                        false,
                    );
                    if maximized {
                        window.maximize();
                    }
                }
            });

            if maximized {
                let window = self.window.clone();
                glib::timeout_add_local_once(Duration::from_millis(150), move || {
                    window.maximize();
                });
            } else {
                glib::timeout_add_local_once(Duration::from_millis(150), move || {
                    restore_window_placement(
                        APP_WINDOW_TITLE,
                        position.x,
                        position.y,
                        width,
                        height,
                        false,
                    );
                });
            }
        }
    }

    pub fn initialize_split_positions(&self, on_settled: impl Fn() + 'static) {
        let horizontal = self.horizontal_paned.clone();
        let vertical = self.vertical_paned.clone();
        glib::timeout_add_local_once(Duration::from_millis(30), move || {
            let horizontal_width = horizontal.width();
            if horizontal_width > 0 {
                horizontal.set_position(horizontal_width / 2);
            }

            let vertical_height = vertical.height();
            if vertical_height > 0 {
                vertical.set_position((vertical_height * 4) / 5);
            }

            glib::timeout_add_local_once(Duration::from_millis(16), move || {
                on_settled();
            });
        });
    }
}

pub(crate) fn persist_pane_session(
    commander: &Commander,
    config_store: &ConfigStore,
) -> anyhow::Result<()> {
    config_store.update(|app_config| {
        write_pane_session(app_config, commander);
    })?;
    Ok(())
}

fn write_pane_session(app_config: &mut AppConfig, commander: &Commander) {
    app_config.panes.left_directory = commander.panel_directory(ActivePanel::Left);
    app_config.panes.right_directory = commander.panel_directory(ActivePanel::Right);
    app_config.panes.left_cursor = commander.cursor_name(ActivePanel::Left);
    app_config.panes.right_cursor = commander.cursor_name(ActivePanel::Right);
    app_config.panes.left_tabs = pane_tabs_config(commander, ActivePanel::Left);
    app_config.panes.right_tabs = pane_tabs_config(commander, ActivePanel::Right);
    app_config.panes.left_active_tab = commander.state().tabs(ActivePanel::Left).active_index();
    app_config.panes.right_active_tab = commander.state().tabs(ActivePanel::Right).active_index();
}

fn pane_tabs_config(
    commander: &crate::application::Commander,
    panel: ActivePanel,
) -> Vec<crate::config::PaneTabConfig> {
    commander
        .state()
        .tabs(panel)
        .tabs()
        .iter()
        .map(|tab| crate::config::PaneTabConfig {
            directory: tab.panel.location.host_directory(),
            cursor: tab.panel.cursor_name(),
            sort_column: tab.panel.sort_state.column,
            sort_direction: tab.panel.sort_state.direction,
        })
        .collect()
}

/// Close every application window except the main commander window.
///
/// Returns `true` when the main window should wait (auxiliary windows are still
/// open, e.g. an editor discard confirmation).
fn close_auxiliary_windows(main: &gtk::ApplicationWindow) -> bool {
    let Some(app) = main.application() else {
        return false;
    };

    let main_window = main.clone().upcast::<gtk::Window>();
    let others: Vec<gtk::Window> = app
        .windows()
        .into_iter()
        .filter(|window| window != &main_window)
        .collect();

    if others.is_empty() {
        return false;
    }

    let pending = Rc::new(Cell::new(others.len()));
    let waiting = Rc::new(Cell::new(false));
    for other in others {
        let pending = Rc::clone(&pending);
        let waiting = Rc::clone(&waiting);
        let main = main.clone();
        other.connect_destroy(move |_| {
            let left = pending.get().saturating_sub(1);
            pending.set(left);
            if left == 0 && waiting.get() {
                let main = main.clone();
                glib::idle_add_local_once(move || {
                    if auxiliary_windows_open(&main) {
                        return;
                    }
                    main.close();
                });
            }
        });
        other.close();
    }

    if pending.get() == 0 {
        return false;
    }

    waiting.set(true);
    true
}

fn auxiliary_windows_open(main: &gtk::ApplicationWindow) -> bool {
    let Some(app) = main.application() else {
        return false;
    };
    let main_window = main.clone().upcast::<gtk::Window>();
    app.windows().into_iter().any(|window| window != main_window)
}
