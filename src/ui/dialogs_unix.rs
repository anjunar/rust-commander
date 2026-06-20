#![cfg_attr(target_os = "windows", allow(dead_code))]

use std::{cell::RefCell, path::PathBuf, rc::Rc};

use gtk::{glib, prelude::*};
use rust_i18n::t;

use super::{
    build_modal_window, dialogs_base::ModalWindow, dialogs_operations::selected_paths_summary,
};

pub fn prompt_unix_chmod<F>(
    parent: &gtk::ApplicationWindow,
    selected_paths: Vec<PathBuf>,
    on_confirm: F,
) where
    F: FnOnce(String, bool) + 'static,
{
    let ModalWindow {
        window,
        content,
        actions,
    } = build_modal_window(parent, &t!("dialog.chmod_title"), 460, 180);

    let label = gtk::Label::new(Some(&t!("dialog.chmod_prompt")));
    label.set_xalign(0.0);
    content.append(&label);

    let summary = gtk::Label::new(Some(&selected_paths_summary(&selected_paths)));
    summary.set_xalign(0.0);
    summary.set_wrap(true);
    summary.add_css_class("dim-label");
    content.append(&summary);

    let mode_entry = gtk::Entry::new();
    mode_entry.set_hexpand(true);
    mode_entry.set_placeholder_text(Some(&t!("dialog.chmod_placeholder")));
    content.append(&mode_entry);

    let recursive_switch = gtk::Switch::new();
    content.append(&switch_row(
        &t!("dialog.recursive_apply"),
        &recursive_switch,
    ));

    let cancel_button = gtk::Button::with_label(&t!("common.cancel"));
    let confirm_button = gtk::Button::with_label(&t!("common.apply"));
    confirm_button.add_css_class("suggested-action");
    actions.append(&cancel_button);
    actions.append(&confirm_button);
    window.set_default_widget(Some(&confirm_button));

    let callback = Rc::new(RefCell::new(Some(on_confirm)));
    {
        let window = window.clone();
        cancel_button.connect_clicked(move |_| {
            window.close();
        });
    }
    {
        let window = window.clone();
        let mode_entry = mode_entry.clone();
        let recursive_switch = recursive_switch.clone();
        let callback = Rc::clone(&callback);
        confirm_button.connect_clicked(move |_| {
            let mode = mode_entry.text().to_string();
            let recursive = recursive_switch.is_active();
            if let Some(on_confirm) = callback.borrow_mut().take() {
                on_confirm(mode, recursive);
            }
            window.close();
        });
    }

    glib::idle_add_local_once(move || {
        window.present();
        mode_entry.grab_focus();
    });
}

pub fn prompt_unix_chown<F>(
    parent: &gtk::ApplicationWindow,
    selected_paths: Vec<PathBuf>,
    on_confirm: F,
) where
    F: FnOnce(String, bool) + 'static,
{
    let ModalWindow {
        window,
        content,
        actions,
    } = build_modal_window(parent, &t!("dialog.chown_title"), 460, 180);

    let label = gtk::Label::new(Some(&t!("dialog.chown_prompt")));
    label.set_xalign(0.0);
    content.append(&label);

    let summary = gtk::Label::new(Some(&selected_paths_summary(&selected_paths)));
    summary.set_xalign(0.0);
    summary.set_wrap(true);
    summary.add_css_class("dim-label");
    content.append(&summary);

    let owner_entry = gtk::Entry::new();
    owner_entry.set_hexpand(true);
    owner_entry.set_placeholder_text(Some(&t!("dialog.chown_placeholder")));
    content.append(&owner_entry);

    let recursive_switch = gtk::Switch::new();
    content.append(&switch_row(
        &t!("dialog.recursive_apply"),
        &recursive_switch,
    ));

    let cancel_button = gtk::Button::with_label(&t!("common.cancel"));
    let confirm_button = gtk::Button::with_label(&t!("common.apply"));
    confirm_button.add_css_class("suggested-action");
    actions.append(&cancel_button);
    actions.append(&confirm_button);
    window.set_default_widget(Some(&confirm_button));

    let callback = Rc::new(RefCell::new(Some(on_confirm)));
    {
        let window = window.clone();
        cancel_button.connect_clicked(move |_| {
            window.close();
        });
    }
    {
        let window = window.clone();
        let owner_entry = owner_entry.clone();
        let recursive_switch = recursive_switch.clone();
        let callback = Rc::clone(&callback);
        confirm_button.connect_clicked(move |_| {
            let owner = owner_entry.text().to_string();
            let recursive = recursive_switch.is_active();
            if let Some(on_confirm) = callback.borrow_mut().take() {
                on_confirm(owner, recursive);
            }
            window.close();
        });
    }

    glib::idle_add_local_once(move || {
        window.present();
        owner_entry.grab_focus();
    });
}

fn settings_row() -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Horizontal, 8)
}

fn row_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label
}

fn switch_row(text: &str, switch: &gtk::Switch) -> gtk::Box {
    let row = settings_row();
    row.append(&row_label(text));
    row.append(switch);
    row
}
