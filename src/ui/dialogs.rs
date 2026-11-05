use gtk::{glib, prelude::*};
use rust_i18n::t;

use crate::ui::dsl::{button, entry, text, Ctx, IntoText, Property};

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

pub(crate) use dialogs_base::{independent_window, modal, present_and_focus, Modal};
pub use dialogs_base::{show_about, show_error};
pub use dialogs_favorites::{show_favorites, FavoritesAction};
#[cfg(target_os = "windows")]
pub use dialogs_operations::offer_elevation;
pub use dialogs_operations::{confirm_operation, show_conflict, ProgressDialog};
pub use dialogs_properties::{show_properties, show_remote_properties};
pub use dialogs_remote::{confirm_ssh_host_key, prompt_remote_connection, RemoteDialogAction};
pub use dialogs_search::{search_go_to_request, show_find_files, SearchDialogAction};
pub use dialogs_settings::show_settings;
#[cfg(not(target_os = "windows"))]
pub use dialogs_unix::{prompt_unix_chmod, prompt_unix_chown};

use dialogs_base::Once;

pub fn prompt_new_directory<F>(parent: &gtk::ApplicationWindow, on_confirm: F)
where
    F: FnOnce(String) + 'static,
{
    prompt_name(
        parent,
        NamePrompt {
            title: &t!("dialog.mkdir_title"),
            prompt: &t!("dialog.mkdir_prompt"),
            confirm_label: &t!("common.create"),
            initial: "",
            select_all: false,
            opens_window: false,
        },
        on_confirm,
    );
}

pub fn prompt_new_file<F>(parent: &gtk::ApplicationWindow, on_confirm: F)
where
    F: FnOnce(String) + 'static,
{
    prompt_name(
        parent,
        NamePrompt {
            title: &t!("dialog.new_file_title"),
            prompt: &t!("dialog.new_file_prompt"),
            confirm_label: &t!("common.create"),
            initial: "untitled.txt",
            select_all: true,
            opens_window: true,
        },
        on_confirm,
    );
}

pub fn prompt_copy_as<F>(parent: &gtk::ApplicationWindow, current_name: String, on_confirm: F)
where
    F: FnOnce(String) + 'static,
{
    prompt_name(
        parent,
        NamePrompt {
            title: &t!("dialog.copy_as_title"),
            prompt: &t!("dialog.copy_as_prompt"),
            confirm_label: &t!("common.copy"),
            initial: &current_name,
            select_all: true,
            opens_window: false,
        },
        on_confirm,
    );
}

struct NamePrompt<'a> {
    title: &'a str,
    prompt: &'a str,
    confirm_label: &'a str,
    initial: &'a str,
    select_all: bool,
    /// Confirming opens another window, which keeps the focus.
    opens_window: bool,
}

/// Asks for one name.
fn prompt_name<F>(parent: &gtk::ApplicationWindow, prompt: NamePrompt, on_confirm: F)
where
    F: FnOnce(String) + 'static,
{
    let name = Property::new(prompt.initial.to_owned());
    let on_confirm = Once::new(on_confirm);

    let (window, (name_entry, confirm)) = modal(parent, prompt.title, (420, 120), |dialog| {
        let name_entry = dialog.content(|| {
            text(prompt.prompt, |_| {});
            entry(&name, |entry| {
                entry.set_hexpand(true);
                entry.set_activates_default(true);
            })
        });
        let confirm = dialog.actions(|| {
            dialog.close_button(t!("common.cancel"));
            confirm_button(
                prompt.confirm_label,
                dialog,
                &name,
                &on_confirm,
                prompt.opens_window,
            )
        });
        (name_entry, confirm)
    });
    window.set_default_widget(Some(confirm.widget()));

    let name_entry = name_entry.widget().clone();
    let select_all = prompt.select_all;
    glib::idle_add_local_once(move || {
        window.present();
        name_entry.grab_focus();
        if select_all {
            name_entry.select_region(0, -1);
        }
    });
}

/// The suggested action of a single-value prompt: hands the value over and closes.
fn confirm_button(
    label: impl IntoText,
    dialog: &Modal,
    value: &Property<String>,
    on_confirm: &Once<String>,
    opens_window: bool,
) -> Ctx<gtk::Button> {
    let value = value.clone();
    let on_confirm = on_confirm.clone();
    let window = dialog.window().widget().clone();
    let skip_parent_restore = dialog.skip_parent_restore();
    button(label, move |button| {
        button.css("suggested-action");
        button.on_click(move || {
            if opens_window {
                // The new window must not fight this one for activation.
                skip_parent_restore.set(true);
                window.close();
                let on_confirm = on_confirm.clone();
                let value = value.get();
                glib::idle_add_local_once(move || on_confirm.call(value));
            } else {
                on_confirm.call(value.get());
                window.close();
            }
        });
    })
}
