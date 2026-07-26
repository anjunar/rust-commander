use std::cell::Cell;
use std::rc::Rc;

use gtk::{gdk, glib, prelude::*};
use rust_i18n::t;

use crate::platform::assets::asset_path;

pub(crate) struct ModalWindow {
    pub window: gtk::Window,
    pub content: gtk::Box,
    pub actions: gtk::Box,
    pub(crate) skip_parent_restore: Rc<Cell<bool>>,
}

pub(crate) struct IndependentWindow {
    pub window: gtk::ApplicationWindow,
    pub header: gtk::HeaderBar,
    pub title_label: gtk::Label,
    pub content: gtk::Box,
    pub actions: gtk::Box,
}

/// Reclaim OS + in-app focus for the main window after a modal closes.
///
/// Must run from `close-request` (while the closing dialog still owns the
/// foreground) so Windows will allow activating the parent. A deferred-only
/// `present()` after destroy often loses to the Alt-Tab previous app.
fn restore_parent_focus(parent: &gtk::ApplicationWindow) {
    parent.present();
    #[cfg(target_os = "windows")]
    if let Some(title) = parent.title() {
        crate::platform::force_foreground_window(&title);
    }

    let parent = parent.clone();
    glib::idle_add_local_once(move || {
        parent.present();
        #[cfg(target_os = "windows")]
        if let Some(title) = parent.title() {
            crate::platform::force_foreground_window(&title);
        }
        let _ = WidgetExt::activate_action(&parent, "focus-active-panel", None);
    });
}

pub(crate) fn build_modal_window(
    parent: &gtk::ApplicationWindow,
    title: &str,
    default_width: i32,
    default_height: i32,
) -> ModalWindow {
    let window = gtk::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title(title)
        .default_width(default_width)
        .default_height(default_height)
        .build();

    if let Some(app) = parent.application() {
        window.set_application(Some(&app));
    }

    let skip_parent_restore = Rc::new(Cell::new(false));
    {
        let parent = parent.clone();
        let skip_parent_restore = Rc::clone(&skip_parent_restore);
        window.connect_close_request(move |_| {
            if !skip_parent_restore.get() {
                restore_parent_focus(&parent);
            }
            glib::Propagation::Proceed
        });
    }

    {
        let key_controller = gtk::EventControllerKey::new();
        key_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        let window_for_esc = window.clone();
        key_controller.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape {
                window_for_esc.close();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        window.add_controller(key_controller);
    }

    #[cfg(target_os = "windows")]
    install_dialog_window_controls(&window, title);

    let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
    root.set_margin_top(12);
    root.set_margin_bottom(14);
    root.set_margin_start(12);
    root.set_margin_end(12);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    actions.set_halign(gtk::Align::End);
    actions.set_margin_top(3);
    actions.set_margin_bottom(0);

    root.append(&content);
    root.append(&actions);
    window.set_child(Some(&root));

    ModalWindow {
        window,
        content,
        actions,
        skip_parent_restore,
    }
}

/// Build a non-modal top-level window that can be Alt-Tab'd independently.
pub(crate) fn build_independent_window(
    parent: &gtk::ApplicationWindow,
    title: &str,
    default_width: i32,
    default_height: i32,
) -> IndependentWindow {
    let window = gtk::ApplicationWindow::builder()
        .title(title)
        .default_width(default_width)
        .default_height(default_height)
        .resizable(true)
        .build();

    if let Some(app) = parent.application() {
        window.set_application(Some(&app));
    }

    let header = gtk::HeaderBar::new();
    let title_label = gtk::Label::new(Some(title));
    title_label.add_css_class("app-title");
    title_label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    title_label.set_hexpand(true);
    title_label.set_xalign(0.0);
    if !title.is_empty() {
        title_label.set_tooltip_text(Some(title));
    }
    header.set_title_widget(Some(&title_label));

    #[cfg(target_os = "windows")]
    {
        install_standard_window_controls(&window, &header);
    }
    #[cfg(not(target_os = "windows"))]
    {
        header.set_show_title_buttons(true);
    }
    window.set_titlebar(Some(&header));

    let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
    root.set_margin_top(12);
    root.set_margin_bottom(14);
    root.set_margin_start(12);
    root.set_margin_end(12);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    actions.set_halign(gtk::Align::End);
    actions.set_margin_top(3);
    actions.set_margin_bottom(0);

    root.append(&content);
    root.append(&actions);
    window.set_child(Some(&root));

    IndependentWindow {
        window,
        header,
        title_label,
        content,
        actions,
    }
}

/// Present a modal and put keyboard focus on the primary action (not Cancel).
pub(crate) fn present_with_focus(window: gtk::Window, focus: &impl IsA<gtk::Widget>) {
    let focus = focus.clone().upcast::<gtk::Widget>();
    glib::idle_add_local_once(move || {
        window.present();
        let _ = focus.grab_focus();
    });
}

#[cfg(target_os = "windows")]
fn install_dialog_window_controls(window: &gtk::Window, title: &str) {
    let header = gtk::HeaderBar::new();
    header.set_show_title_buttons(false);

    let title_label = gtk::Label::new(Some(title));
    title_label.add_css_class("app-title");
    header.set_title_widget(Some(&title_label));

    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    controls.add_css_class("window-controls");
    controls.append(&close_window_button(window));

    header.pack_end(&controls);
    window.set_titlebar(Some(&header));
}

#[cfg(target_os = "windows")]
fn install_standard_window_controls(
    window: &gtk::ApplicationWindow,
    header: &gtk::HeaderBar,
) {
    header.set_show_title_buttons(false);

    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    controls.add_css_class("window-controls");

    let minimize_button = gtk::Button::from_icon_name("window-minimize-symbolic");
    minimize_button.add_css_class("window-control-button");
    minimize_button.add_css_class("window-minimize-button");
    minimize_button.add_css_class("flat");
    minimize_button.set_focus_on_click(false);
    minimize_button.set_size_request(36, 24);
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
    maximize_button.set_size_request(36, 24);
    maximize_button.set_tooltip_text(Some("Maximize"));
    sync_maximize_button(window, &maximize_button);
    {
        let window = window.clone();
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
    controls.append(&close_window_button(window.upcast_ref()));

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

#[cfg(target_os = "windows")]
fn close_window_button(window: &gtk::Window) -> gtk::Button {
    let close_button = gtk::Button::from_icon_name("window-close-symbolic");
    close_button.add_css_class("window-control-button");
    close_button.add_css_class("window-close-button");
    close_button.add_css_class("flat");
    close_button.set_focus_on_click(false);
    close_button.set_size_request(36, 24);
    close_button.set_tooltip_text(Some("Close"));
    {
        let window = window.clone();
        close_button.connect_clicked(move |_| {
            window.close();
        });
    }
    close_button
}

pub fn show_error(parent: &gtk::ApplicationWindow, title: &str, detail: &str) {
    let ModalWindow {
        window,
        content,
        actions,
        ..
    } = build_modal_window(parent, title, 460, 180);

    let title_label = gtk::Label::new(Some(title));
    title_label.set_xalign(0.0);
    title_label.set_wrap(true);
    title_label.add_css_class("dialog-title");
    content.append(&title_label);

    let detail_label = gtk::Label::new(Some(detail));
    detail_label.set_xalign(0.0);
    detail_label.set_wrap(true);
    content.append(&detail_label);

    let close_button = gtk::Button::with_label(&t!("common.close"));
    close_button.add_css_class("suggested-action");
    actions.append(&close_button);
    window.set_default_widget(Some(&close_button));

    let window_for_close = window.clone();
    close_button.connect_clicked(move |_| {
        window_for_close.close();
    });

    present_with_focus(window, &close_button);
}

pub fn show_about(parent: &gtk::ApplicationWindow) {
    let dialog = gtk::AboutDialog::builder()
        .transient_for(parent)
        .modal(true)
        .program_name("RCommander")
        .version(env!("CARGO_PKG_VERSION"))
        .comments(t!("dialog.about_comments").as_ref())
        .copyright("License: MIT")
        .license_type(gtk::License::MitX11)
        .build();
    let logo_path = asset_path("assets/icons/rust-commander.png");
    if let Ok(texture) = gdk::Texture::from_filename(&logo_path) {
        dialog.set_logo(Some(&texture));
    } else {
        dialog.set_logo_icon_name(Some("rust-commander"));
    }
    {
        let parent = parent.clone();
        dialog.connect_close_request(move |_| {
            restore_parent_focus(&parent);
            glib::Propagation::Proceed
        });
    }
    dialog.present();
}
