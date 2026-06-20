use std::{cell::RefCell, rc::Rc, time::Duration};

use gtk::{glib, prelude::*};

#[cfg(target_os = "windows")]
use crate::platform::restore_window_placement;

use crate::{
    application::{ActivePanel, Commander, ConfigStore},
    config::{WindowConfig, WindowPosition},
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
        let window = self.window.clone();
        let config_store = self.config_store.clone();
        self.window.connect_close_request(move |_| {
            if let Err(error) = config_store.update(|app_config| {
                let commander = commander.borrow();
                app_config.window.maximized = window.is_maximized();
                if !app_config.window.maximized {
                    app_config.window.width = window.width().max(1);
                    app_config.window.height = window.height().max(1);
                }
                app_config.panes.left_directory = commander.panel_directory(ActivePanel::Left);
                app_config.panes.right_directory = commander.panel_directory(ActivePanel::Right);
            }) {
                eprintln!("Could not save config: {error}");
            }
            glib::Propagation::Proceed
        });
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

    pub fn initialize_split_positions(&self) {
        let horizontal = self.horizontal_paned.clone();
        let vertical = self.vertical_paned.clone();
        glib::timeout_add_local_once(Duration::from_millis(30), move || {
            let horizontal_width = horizontal.width();
            if horizontal_width > 0 {
                horizontal.set_position(horizontal_width / 2);
            }

            let vertical_height = vertical.height();
            if vertical_height > 0 {
                vertical.set_position(vertical_height / 2);
            }
        });
    }
}
