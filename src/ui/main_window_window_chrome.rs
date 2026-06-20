use std::{cell::RefCell, rc::Rc};

use gtk::prelude::*;

use crate::{
    config::AppConfig,
    ui::{commander_view::CommanderView, terminal_dock::TerminalDock, theme::ThemeController},
};

use super::{command_bar::command_bar_labels, APP_WINDOW_TITLE};

#[derive(Clone)]
pub struct WindowChromeController {
    window: gtk::ApplicationWindow,
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
        app_config_cache: Rc<RefCell<AppConfig>>,
        theme_controller: Rc<ThemeController>,
    ) -> Self {
        Self {
            window,
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
