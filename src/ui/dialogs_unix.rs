use std::path::PathBuf;

use gtk::prelude::*;
use rust_i18n::t;

use crate::ui::dsl::{button, text, Property};

use super::{
    dialogs_base::{field, hint, modal, present_and_focus, switch_row, Once},
    dialogs_operations::selected_paths_summary,
};

pub fn prompt_unix_chmod<F>(
    parent: &gtk::ApplicationWindow,
    selected_paths: Vec<PathBuf>,
    on_confirm: F,
) where
    F: FnOnce(String, bool) + 'static,
{
    prompt_unix_value(
        parent,
        &t!("dialog.chmod_title"),
        &t!("dialog.chmod_prompt"),
        &t!("dialog.chmod_placeholder"),
        selected_paths,
        on_confirm,
    );
}

pub fn prompt_unix_chown<F>(
    parent: &gtk::ApplicationWindow,
    selected_paths: Vec<PathBuf>,
    on_confirm: F,
) where
    F: FnOnce(String, bool) + 'static,
{
    prompt_unix_value(
        parent,
        &t!("dialog.chown_title"),
        &t!("dialog.chown_prompt"),
        &t!("dialog.chown_placeholder"),
        selected_paths,
        on_confirm,
    );
}

/// One value (a mode or an owner) plus a recursive switch, applied to the selection.
fn prompt_unix_value<F>(
    parent: &gtk::ApplicationWindow,
    title: &str,
    prompt: &str,
    placeholder: &str,
    selected_paths: Vec<PathBuf>,
    on_confirm: F,
) where
    F: FnOnce(String, bool) + 'static,
{
    let value = Property::new(String::new());
    let recursive = Property::new(false);
    let on_confirm = Once::new(move |(value, recursive)| on_confirm(value, recursive));

    let (window, (value_entry, apply)) = modal(parent, title, (460, 180), |dialog| {
        let value_entry = dialog.content(|| {
            text(prompt, |_| {});
            hint(selected_paths_summary(&selected_paths));
            let value_entry = field(placeholder, &value);
            switch_row(t!("dialog.recursive_apply"), &recursive);
            value_entry
        });
        let apply = dialog.actions(|| {
            dialog.close_button(t!("common.cancel"));
            button(t!("common.apply"), |button| {
                button.css("suggested-action");
                let value = value.clone();
                let recursive = recursive.clone();
                let window = dialog.window().widget().clone();
                let on_confirm = on_confirm.clone();
                button.on_click(move || {
                    on_confirm.call((value.get(), recursive.get()));
                    window.close();
                });
            })
        });
        (value_entry, apply)
    });
    window.set_default_widget(Some(apply.widget()));
    present_and_focus(&window, value_entry.widget());
}
