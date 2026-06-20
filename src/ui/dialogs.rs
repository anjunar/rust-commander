use std::{cell::RefCell, path::PathBuf, rc::Rc};

use gtk::{glib, prelude::*};
use rust_i18n::t;

#[path = "dialogs_base.rs"]
mod dialogs_base;
#[path = "dialogs_operations.rs"]
mod dialogs_operations;
#[path = "dialogs_remote.rs"]
mod dialogs_remote;
#[path = "dialogs_settings.rs"]
mod dialogs_settings;
#[cfg(not(target_os = "windows"))]
#[path = "dialogs_unix.rs"]
mod dialogs_unix;

pub use dialogs_base::show_error;
pub use dialogs_operations::{confirm_operation, show_conflict, ProgressDialog};
pub use dialogs_remote::{prompt_remote_connection, RemoteDialogAction};
pub use dialogs_settings::show_settings;
#[cfg(not(target_os = "windows"))]
pub use dialogs_unix::{prompt_unix_chmod, prompt_unix_chown};

pub(crate) use dialogs_base::build_modal_window;
use dialogs_base::ModalWindow;
pub fn prompt_archive_open_action<F>(
    parent: &gtk::ApplicationWindow,
    archive_path: PathBuf,
    on_choice: F,
) where
    F: FnOnce(bool) + 'static,
{
    let title = t!("dialog.archive_open_title").into_owned();
    let detail = t!(
        "dialog.archive_open_detail",
        path = archive_path.display().to_string()
    )
    .into_owned();

    let ModalWindow {
        window,
        content,
        actions,
    } = build_modal_window(parent, &title, 500, 180);

    let title_label = gtk::Label::new(Some(&title));
    title_label.set_xalign(0.0);
    title_label.set_wrap(true);
    title_label.add_css_class("dialog-title");
    content.append(&title_label);

    let detail_label = gtk::Label::new(Some(&detail));
    detail_label.set_xalign(0.0);
    detail_label.set_wrap(true);
    content.append(&detail_label);

    let cancel_button = gtk::Button::with_label(&t!("common.cancel"));
    let external_button = gtk::Button::with_label(&t!("dialog.archive_open_external"));
    let archive_button = gtk::Button::with_label(&t!("dialog.archive_open_internal"));
    archive_button.add_css_class("suggested-action");
    actions.append(&cancel_button);
    actions.append(&external_button);
    actions.append(&archive_button);
    window.set_default_widget(Some(&archive_button));

    {
        let window = window.clone();
        cancel_button.connect_clicked(move |_| {
            window.close();
        });
    }

    let callback = Rc::new(RefCell::new(Some(on_choice)));
    {
        let window = window.clone();
        let callback = Rc::clone(&callback);
        archive_button.connect_clicked(move |_| {
            if let Some(on_choice) = callback.borrow_mut().take() {
                on_choice(true);
            }
            window.close();
        });
    }
    {
        let window = window.clone();
        let callback = Rc::clone(&callback);
        external_button.connect_clicked(move |_| {
            if let Some(on_choice) = callback.borrow_mut().take() {
                on_choice(false);
            }
            window.close();
        });
    }

    window.present();
}

pub fn prompt_rename<F>(parent: &gtk::ApplicationWindow, current_name: String, on_confirm: F)
where
    F: FnOnce(String) + 'static,
{
    let ModalWindow {
        window,
        content,
        actions,
    } = build_modal_window(parent, &t!("dialog.rename_title"), 420, 120);

    let label = gtk::Label::new(Some(&t!("dialog.rename_prompt")));
    label.set_xalign(0.0);
    content.append(&label);

    let entry = gtk::Entry::new();
    entry.set_text(&current_name);
    entry.set_hexpand(true);
    content.append(&entry);

    let cancel_button = gtk::Button::with_label(&t!("common.cancel"));
    let confirm_button = gtk::Button::with_label(&t!("common.rename"));
    confirm_button.add_css_class("suggested-action");
    actions.append(&cancel_button);
    actions.append(&confirm_button);
    window.set_default_widget(Some(&confirm_button));

    let callback = std::rc::Rc::new(std::cell::RefCell::new(Some(on_confirm)));
    {
        let window = window.clone();
        cancel_button.connect_clicked(move |_| {
            window.close();
        });
    }
    {
        let window = window.clone();
        let entry = entry.clone();
        let callback = std::rc::Rc::clone(&callback);
        confirm_button.connect_clicked(move |_| {
            let value = entry.text().to_string();
            if let Some(on_confirm) = callback.borrow_mut().take() {
                on_confirm(value);
            }
            window.close();
        });
    }

    glib::idle_add_local_once(move || {
        window.present();
        entry.grab_focus();
        entry.select_region(0, -1);
    });
}

pub fn prompt_new_directory<F>(parent: &gtk::ApplicationWindow, on_confirm: F)
where
    F: FnOnce(String) + 'static,
{
    let ModalWindow {
        window,
        content,
        actions,
    } = build_modal_window(parent, &t!("dialog.mkdir_title"), 420, 120);

    let label = gtk::Label::new(Some(&t!("dialog.mkdir_prompt")));
    label.set_xalign(0.0);
    content.append(&label);

    let entry = gtk::Entry::new();
    entry.set_hexpand(true);
    content.append(&entry);

    let cancel_button = gtk::Button::with_label(&t!("common.cancel"));
    let confirm_button = gtk::Button::with_label(&t!("common.create"));
    confirm_button.add_css_class("suggested-action");
    actions.append(&cancel_button);
    actions.append(&confirm_button);
    window.set_default_widget(Some(&confirm_button));

    let callback = std::rc::Rc::new(std::cell::RefCell::new(Some(on_confirm)));
    {
        let window = window.clone();
        cancel_button.connect_clicked(move |_| {
            window.close();
        });
    }
    {
        let window = window.clone();
        let entry = entry.clone();
        let callback = std::rc::Rc::clone(&callback);
        confirm_button.connect_clicked(move |_| {
            let value = entry.text().to_string();
            if let Some(on_confirm) = callback.borrow_mut().take() {
                on_confirm(value);
            }
            window.close();
        });
    }

    glib::idle_add_local_once(move || {
        window.present();
        entry.grab_focus();
    });
}
