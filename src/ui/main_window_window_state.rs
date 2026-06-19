use std::{cell::RefCell, rc::Rc, time::Duration};

use gtk::{glib, prelude::*};

#[cfg(target_os = "windows")]
use crate::platform::restore_window_placement;

use crate::{
    application::{ActivePanel, Commander},
    config::{self, AppConfig, WindowConfig, WindowPosition},
    platform::current_window_placement,
};

use super::APP_WINDOW_TITLE;

pub struct WindowStateController {
    window: gtk::ApplicationWindow,
    horizontal_paned: gtk::Paned,
    vertical_paned: gtk::Paned,
    commander: Rc<RefCell<Commander>>,
    app_config_cache: Rc<RefCell<AppConfig>>,
}

impl WindowStateController {
    pub fn new(
        window: gtk::ApplicationWindow,
        horizontal_paned: gtk::Paned,
        vertical_paned: gtk::Paned,
        commander: Rc<RefCell<Commander>>,
        app_config_cache: Rc<RefCell<AppConfig>>,
    ) -> Self {
        Self {
            window,
            horizontal_paned,
            vertical_paned,
            commander,
            app_config_cache,
        }
    }

    pub fn install_window_state_persistence(&self) {
        let commander = Rc::clone(&self.commander);
        let window = self.window.clone();
        let app_config_cache = Rc::clone(&self.app_config_cache);
        self.window.connect_close_request(move |_| {
            {
                let commander = commander.borrow();
                let mut app_config = app_config_cache.borrow_mut();
                app_config.window.maximized = window.is_maximized();
                if !app_config.window.maximized {
                    app_config.window.width = window.width().max(1);
                    app_config.window.height = window.height().max(1);
                }
                app_config.panes.left_directory =
                    Some(commander.panel_directory(ActivePanel::Left));
                app_config.panes.right_directory =
                    Some(commander.panel_directory(ActivePanel::Right));
            }

            if let Err(error) = config::save(&app_config_cache.borrow().clone()) {
                eprintln!("Could not save config: {error}");
            }
            glib::Propagation::Proceed
        });
    }

    pub fn install_window_geometry_tracking(&self) {
        let window = self.window.clone();
        let app_config_cache = Rc::clone(&self.app_config_cache);
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
            glib::idle_add_local_once({
                let position = position.clone();
                let width = window_config.width;
                let height = window_config.height;
                let maximized = window_config.maximized;
                move || {
                    restore_window_placement(
                        APP_WINDOW_TITLE,
                        position.x,
                        position.y,
                        width,
                        height,
                        maximized,
                    );
                }
            });
            let width = window_config.width;
            let height = window_config.height;
            let maximized = window_config.maximized;
            glib::timeout_add_local_once(Duration::from_millis(150), move || {
                restore_window_placement(
                    APP_WINDOW_TITLE,
                    position.x,
                    position.y,
                    width,
                    height,
                    maximized,
                );
            });
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
