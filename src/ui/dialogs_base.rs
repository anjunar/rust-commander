use gtk::{glib, prelude::*};
use rust_i18n::t;

pub(crate) struct ModalWindow {
    pub window: gtk::Window,
    pub content: gtk::Box,
    pub actions: gtk::Box,
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

    {
        let parent = parent.clone();
        window.connect_close_request(move |_| {
            let parent = parent.clone();
            glib::idle_add_local_once(move || {
                parent.present();
                parent.grab_focus();
            });
            glib::Propagation::Proceed
        });
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
    }
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
    window.set_titlebar(Some(&header));
}

pub fn show_error(parent: &gtk::ApplicationWindow, title: &str, detail: &str) {
    let ModalWindow {
        window,
        content,
        actions,
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

    window.present();
}
