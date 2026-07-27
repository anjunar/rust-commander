use std::rc::Rc;

use gtk::{glib, prelude::*};
use rust_i18n::t;

#[path = "dialogs_base.rs"]
mod dialogs_base;
#[path = "dialogs_favorites.rs"]
mod dialogs_favorites;
#[path = "dialogs_operations.rs"]
mod dialogs_operations;
#[path = "dialogs_properties.rs"]
mod dialogs_properties;
#[path = "dialogs_remote.rs"]
mod dialogs_remote;
#[path = "dialogs_search.rs"]
mod dialogs_search;
#[path = "dialogs_settings.rs"]
mod dialogs_settings;
#[cfg(not(target_os = "windows"))]
#[path = "dialogs_unix.rs"]
mod dialogs_unix;

pub use dialogs_base::{show_about, show_error};
pub use dialogs_favorites::{show_favorites, FavoritesAction};
pub use dialogs_operations::{confirm_operation, show_conflict, ProgressDialog};
pub use dialogs_properties::{show_properties, show_remote_properties};
pub use dialogs_remote::{confirm_ssh_host_key, prompt_remote_connection, RemoteDialogAction};
pub use dialogs_search::{search_go_to_request, show_find_files, SearchDialogAction};
pub use dialogs_settings::show_settings;
#[cfg(not(target_os = "windows"))]
pub use dialogs_unix::{prompt_unix_chmod, prompt_unix_chown};

pub(crate) use dialogs_base::{build_independent_window, build_modal_window};

pub fn prompt_new_directory<F>(parent: &gtk::ApplicationWindow, on_confirm: F)
where
    F: FnOnce(String) + 'static,
{
    prompt_name(
        parent,
        &t!("dialog.mkdir_title"),
        &t!("dialog.mkdir_prompt"),
        &t!("common.create"),
        None,
        false,
        false,
        on_confirm,
    );
}

pub fn prompt_new_file<F>(parent: &gtk::ApplicationWindow, on_confirm: F)
where
    F: FnOnce(String) + 'static,
{
    prompt_name(
        parent,
        &t!("dialog.new_file_title"),
        &t!("dialog.new_file_prompt"),
        &t!("common.create"),
        Some("untitled.txt"),
        true,
        true,
        on_confirm,
    );
}

pub fn prompt_copy_as<F>(parent: &gtk::ApplicationWindow, current_name: String, on_confirm: F)
where
    F: FnOnce(String) + 'static,
{
    prompt_name(
        parent,
        &t!("dialog.copy_as_title"),
        &t!("dialog.copy_as_prompt"),
        &t!("common.copy"),
        Some(&current_name),
        true,
        false,
        on_confirm,
    );
}

fn prompt_name<F>(
    parent: &gtk::ApplicationWindow,
    title: &str,
    prompt: &str,
    confirm_label: &str,
    initial: Option<&str>,
    select_all: bool,
    skip_parent_restore_on_confirm: bool,
    on_confirm: F,
) where
    F: FnOnce(String) + 'static,
{
    let modal = build_modal_window(parent, title, 420, 120);

    let label = gtk::Label::new(Some(prompt));
    label.set_xalign(0.0);
    modal.content.append(&label);

    let entry = gtk::Entry::new();
    if let Some(initial) = initial {
        entry.set_text(initial);
    }
    entry.set_hexpand(true);
    entry.set_activates_default(true);
    modal.content.append(&entry);

    let cancel_button = gtk::Button::with_label(&t!("common.cancel"));
    let confirm_button = gtk::Button::with_label(confirm_label);
    confirm_button.add_css_class("suggested-action");
    modal.actions.append(&cancel_button);
    modal.actions.append(&confirm_button);
    modal.window.set_default_widget(Some(&confirm_button));

    let callback = Rc::new(std::cell::RefCell::new(Some(on_confirm)));
    {
        let window = modal.window.clone();
        cancel_button.connect_clicked(move |_| {
            window.close();
        });
    }
    {
        let window = modal.window.clone();
        let entry = entry.clone();
        let callback = Rc::clone(&callback);
        let skip_parent_restore = Rc::clone(&modal.skip_parent_restore);
        confirm_button.connect_clicked(move |_| {
            let value = entry.text().to_string();
            let on_confirm = callback.borrow_mut().take();
            if skip_parent_restore_on_confirm {
                skip_parent_restore.set(true);
                // Close first, then open the follow-up window on the next idle so the
                // editor is not fighting the modal/transient parent activation.
                window.close();
                if let Some(on_confirm) = on_confirm {
                    glib::idle_add_local_once(move || on_confirm(value));
                }
            } else if let Some(on_confirm) = on_confirm {
                on_confirm(value);
                window.close();
            } else {
                window.close();
            }
        });
    }

    glib::idle_add_local_once(move || {
        modal.window.present();
        entry.grab_focus();
        if select_all {
            entry.select_region(0, -1);
        }
    });
}
