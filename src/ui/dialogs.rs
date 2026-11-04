use std::path::PathBuf;

use gtk::{glib, prelude::*};
use rust_i18n::t;

use crate::ui::dsl::{button, entry, text, Property};

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
pub(crate) use dialogs_base::{modal, present_and_focus, Modal};
pub use dialogs_operations::{confirm_operation, show_conflict, ProgressDialog};
pub use dialogs_remote::{prompt_remote_connection, RemoteDialogAction};
pub use dialogs_settings::show_settings;
#[cfg(not(target_os = "windows"))]
pub use dialogs_unix::{prompt_unix_chmod, prompt_unix_chown};

use dialogs_base::{heading, paragraph, Once};

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
    let on_choice = Once::new(on_choice);

    let (window, open_inside) = modal(parent, &title, (500, 180), |dialog| {
        let choose = |internal: bool| {
            let on_choice = on_choice.clone();
            let window = dialog.window().widget().clone();
            move || {
                on_choice.call(internal);
                window.close();
            }
        };

        dialog.content(|| {
            heading(title.as_str());
            paragraph(detail);
        });
        dialog.actions(|| {
            dialog.close_button(t!("common.cancel"));
            button(t!("dialog.archive_open_external"), |button| {
                button.on_click(choose(false));
            });
            button(t!("dialog.archive_open_internal"), |button| {
                button.css("suggested-action");
                button.on_click(choose(true));
            })
        })
    });
    window.set_default_widget(Some(open_inside.widget()));
    window.present();
}

pub fn prompt_rename<F>(parent: &gtk::ApplicationWindow, current_name: String, on_confirm: F)
where
    F: FnOnce(String) + 'static,
{
    let name = Property::new(current_name);
    let on_confirm = Once::new(on_confirm);

    let (window, (name_entry, confirm)) =
        modal(parent, &t!("dialog.rename_title"), (420, 120), |dialog| {
            let name_entry = dialog.content(|| {
                text(t!("dialog.rename_prompt"), |_| {});
                entry(&name, |entry| entry.set_hexpand(true))
            });
            let confirm = dialog.actions(|| {
                dialog.close_button(t!("common.cancel"));
                confirm_button(t!("common.rename"), dialog, &name, &on_confirm)
            });
            (name_entry, confirm)
        });
    window.set_default_widget(Some(confirm.widget()));

    let name_entry = name_entry.widget().clone();
    glib::idle_add_local_once(move || {
        window.present();
        name_entry.grab_focus();
        name_entry.select_region(0, -1);
    });
}

pub fn prompt_new_directory<F>(parent: &gtk::ApplicationWindow, on_confirm: F)
where
    F: FnOnce(String) + 'static,
{
    let name = Property::new(String::new());
    let on_confirm = Once::new(on_confirm);

    let (window, (name_entry, confirm)) =
        modal(parent, &t!("dialog.mkdir_title"), (420, 120), |dialog| {
            let name_entry = dialog.content(|| {
                text(t!("dialog.mkdir_prompt"), |_| {});
                entry(&name, |entry| entry.set_hexpand(true))
            });
            let confirm = dialog.actions(|| {
                dialog.close_button(t!("common.cancel"));
                confirm_button(t!("common.create"), dialog, &name, &on_confirm)
            });
            (name_entry, confirm)
        });
    window.set_default_widget(Some(confirm.widget()));
    present_and_focus(&window, name_entry.widget());
}

/// The suggested action of a single-value prompt: hands the value over and closes.
fn confirm_button(
    label: impl crate::ui::dsl::IntoText,
    dialog: &Modal,
    value: &Property<String>,
    on_confirm: &Once<String>,
) -> crate::ui::dsl::Ctx<gtk::Button> {
    let value = value.clone();
    let on_confirm = on_confirm.clone();
    let window = dialog.window().widget().clone();
    button(label, move |button| {
        button.css("suggested-action");
        button.on_click(move || {
            on_confirm.call(value.get());
            window.close();
        });
    })
}
