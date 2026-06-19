use std::{cell::RefCell, rc::Rc, time::Duration};

use gtk::{glib, prelude::*};

#[cfg(target_os = "windows")]
use crate::platform::restore_window_placement;

use crate::{
    application::{ActivePanel, Commander},
    config::{self, AppConfig, WindowConfig, WindowPosition},
    platform::current_window_placement,
    ui::{commander_view::CommanderView, terminal_dock::TerminalDock, theme::ThemeController},
};

use super::{command_bar_labels, APP_WINDOW_TITLE};

pub struct WindowChromeController {
    window: gtk::ApplicationWindow,
    horizontal_paned: gtk::Paned,
    vertical_paned: gtk::Paned,
    commander: Rc<RefCell<Commander>>,
    app_config_cache: Rc<RefCell<AppConfig>>,
    theme_controller: Rc<ThemeController>,
}

#[cfg(target_os = "windows")]
pub fn install_custom_window_controls(window: &gtk::ApplicationWindow, header: &gtk::HeaderBar) {
    header.set_show_title_buttons(false);

    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    controls.add_css_class("window-controls");

    let minimize_button = gtk::Button::from_icon_name("window-minimize-symbolic");
    minimize_button.add_css_class("window-control-button");
    minimize_button.add_css_class("window-minimize-button");
    minimize_button.add_css_class("flat");
    minimize_button.set_focus_on_click(false);
    minimize_button.set_size_request(44, 28);
    minimize_button.set_tooltip_text(Some("Minimize"));
    {
        let window = window.clone();
        minimize_button.connect_clicked(move |_| {
            window.minimize();
        });
    }
    controls.append(&minimize_button);

    let maximize_button = gtk::Button::new();
    maximize_button.add_css_class("window-control-button");
    maximize_button.add_css_class("window-maximize-button");
    maximize_button.add_css_class("flat");
    maximize_button.set_focus_on_click(false);
    maximize_button.set_size_request(44, 28);
    maximize_button.set_tooltip_text(Some("Maximize"));
    sync_maximize_button(window, &maximize_button);
    {
        let window = window.clone();
        let maximize_button = maximize_button.clone();
        maximize_button.connect_clicked(move |_| {
            if window.is_maximized() {
                window.unmaximize();
            } else {
                window.maximize();
            }
        });
    }
    {
        let window = window.clone();
        let maximize_button = maximize_button.clone();
        window.connect_maximized_notify(move |window| {
            sync_maximize_button(window, &maximize_button);
        });
    }
    controls.append(&maximize_button);

    let close_button = gtk::Button::from_icon_name("window-close-symbolic");
    close_button.add_css_class("window-control-button");
    close_button.add_css_class("window-close-button");
    close_button.add_css_class("flat");
    close_button.set_focus_on_click(false);
    close_button.set_size_request(44, 28);
    close_button.set_tooltip_text(Some("Close"));
    {
        let window = window.clone();
        close_button.connect_clicked(move |_| {
            window.close();
        });
    }
    controls.append(&close_button);

    header.pack_end(&controls);
}

#[cfg(target_os = "windows")]
fn sync_maximize_button(window: &gtk::ApplicationWindow, button: &gtk::Button) {
    if window.is_maximized() {
        button.set_icon_name("window-restore-symbolic");
        button.set_tooltip_text(Some("Restore"));
    } else {
        button.set_icon_name("window-maximize-symbolic");
        button.set_tooltip_text(Some("Maximize"));
    }
}

impl WindowChromeController {
    pub fn new(
        window: gtk::ApplicationWindow,
        horizontal_paned: gtk::Paned,
        vertical_paned: gtk::Paned,
        commander: Rc<RefCell<Commander>>,
        app_config_cache: Rc<RefCell<AppConfig>>,
        theme_controller: Rc<ThemeController>,
    ) -> Self {
        Self {
            window,
            horizontal_paned,
            vertical_paned,
            commander,
            app_config_cache,
            theme_controller,
        }
    }

    pub fn apply_theme(&self) {
        let preference = self.app_config_cache.borrow().general.theme;
        self.theme_controller.apply(preference);
    }

    pub fn install_system_theme_tracking(&self) {
        let Some(settings) = gtk::Settings::default() else {
            return;
        };

        let app_config_cache = Rc::clone(&self.app_config_cache);
        let theme_controller = Rc::clone(&self.theme_controller);
        settings.connect_gtk_application_prefer_dark_theme_notify(move |_| {
            if app_config_cache.borrow().general.theme == crate::config::ThemePreference::System {
                let preference = app_config_cache.borrow().general.theme;
                theme_controller.apply(preference);
            }
        });
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

    pub fn refresh_localized_labels(
        &self,
        commander_view: &CommanderView,
        terminal_dock: &TerminalDock,
    ) {
        commander_view.refresh_labels();
        terminal_dock.refresh_toolbar();
        if let Some(titlebar) = self.window.titlebar() {
            if let Ok(header) = titlebar.downcast::<gtk::HeaderBar>() {
                if let Some(title_widget) = header.title_widget() {
                    if let Ok(label) = title_widget.downcast::<gtk::Label>() {
                        label.set_label(APP_WINDOW_TITLE);
                    }
                }
            }
        }

        let labels = command_bar_labels();
        let mut index = 0usize;
        let mut child = self
            .window
            .child()
            .and_then(|child| child.downcast::<gtk::Box>().ok())
            .and_then(|shell| shell.last_child());
        while let Some(widget) = child {
            let previous = widget.prev_sibling();
            if let Ok(button_row) = widget.clone().downcast::<gtk::Box>() {
                let mut button = button_row.first_child();
                while let Some(widget) = button {
                    button = widget.next_sibling();
                    if let Ok(button) = widget.downcast::<gtk::Button>() {
                        if let Some(label) = labels.get(index) {
                            button.set_label(label);
                        }
                        index += 1;
                    }
                }
                break;
            }
            child = previous;
        }
    }
}
