use std::{
    cell::{Cell, RefCell},
    path::{Path, PathBuf},
    rc::Rc,
};

use gtk::{glib, prelude::*};
use rust_i18n::t;

use crate::config::FavoriteEntry;

use super::dialogs_base::{build_modal_window, present_with_focus, show_error, ModalWindow};

pub enum FavoritesAction {
    Go(PathBuf),
    Update(Vec<FavoriteEntry>),
}

pub fn show_favorites<F>(
    parent: &gtk::ApplicationWindow,
    favorites: Vec<FavoriteEntry>,
    current_directory: Option<PathBuf>,
    on_action: F,
) where
    F: Fn(FavoritesAction) + 'static,
{
    let ModalWindow {
        window,
        content,
        actions,
    } = build_modal_window(parent, &t!("dialog.favorites_title"), 560, 420);

    let title = gtk::Label::new(Some(&t!("dialog.favorites_title")));
    title.set_xalign(0.0);
    title.add_css_class("dialog-title");
    content.append(&title);

    let hint = gtk::Label::new(Some(&t!("dialog.favorites_hint")));
    hint.set_xalign(0.0);
    hint.set_wrap(true);
    content.append(&hint);

    let state = Rc::new(RefCell::new(favorites));
    let callback: Rc<dyn Fn(FavoritesAction)> = Rc::new(on_action);
    let selected_index = Rc::new(Cell::new(None::<usize>));
    let ignore_selection = Rc::new(Cell::new(false));

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.set_vexpand(true);
    list.add_css_class("favorites-list");

    let scrolled = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&list)
        .build();
    scrolled.add_css_class("panel-scroller");
    content.append(&scrolled);

    let refresh_list = {
        let list = list.clone();
        let state = Rc::clone(&state);
        let selected_index = Rc::clone(&selected_index);
        let ignore_selection = Rc::clone(&ignore_selection);
        Rc::new(move || {
            ignore_selection.set(true);
            while let Some(row) = list.row_at_index(0) {
                list.remove(&row);
            }

            let favorites = state.borrow().clone();
            for entry in &favorites {
                list.append(&favorite_row(entry));
            }

            let next_selected = match selected_index.get() {
                Some(index) if index < favorites.len() => Some(index),
                Some(_) if !favorites.is_empty() => Some(favorites.len() - 1),
                None if !favorites.is_empty() => Some(0),
                _ => None,
            };
            selected_index.set(next_selected);

            if let Some(index) = next_selected {
                if let Some(row) = list.row_at_index(index as i32) {
                    list.select_row(Some(&row));
                }
            }
            ignore_selection.set(false);
        })
    };
    refresh_list();

    {
        let selected_index = Rc::clone(&selected_index);
        let ignore_selection = Rc::clone(&ignore_selection);
        list.connect_row_selected(move |_, row| {
            if ignore_selection.get() {
                return;
            }
            selected_index.set(row.map(|row| row.index() as usize));
        });
    }

    let persist = {
        let state = Rc::clone(&state);
        let callback = Rc::clone(&callback);
        Rc::new(move || {
            callback(FavoritesAction::Update(state.borrow().clone()));
        })
    };

    let go_selected = {
        let window = window.clone();
        let state = Rc::clone(&state);
        let selected_index = Rc::clone(&selected_index);
        let callback = Rc::clone(&callback);
        let parent = parent.clone();
        Rc::new(move || {
            let Some(index) = selected_index.get() else {
                show_error(
                    &parent,
                    &t!("error.favorites_unavailable"),
                    &t!("error.no_favorite_selected"),
                );
                return;
            };
            let Some(entry) = state.borrow().get(index).cloned() else {
                return;
            };
            if !entry.path.is_dir() {
                show_error(
                    &parent,
                    &t!("error.favorites_unavailable"),
                    &t!(
                        "error.favorite_path_missing",
                        path = entry.path.display().to_string()
                    ),
                );
                return;
            }
            callback(FavoritesAction::Go(entry.path));
            window.close();
        })
    };

    {
        let go_selected = Rc::clone(&go_selected);
        list.connect_row_activated(move |_, _| {
            go_selected();
        });
    }

    let button_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    button_row.set_halign(gtk::Align::Start);
    content.append(&button_row);

    let add_button = gtk::Button::with_label(&t!("dialog.favorites_add"));
    let edit_button = gtk::Button::with_label(&t!("dialog.favorites_edit"));
    let delete_button = gtk::Button::with_label(&t!("dialog.favorites_delete"));
    let move_up_button = gtk::Button::with_label(&t!("dialog.favorites_move_up"));
    let move_down_button = gtk::Button::with_label(&t!("dialog.favorites_move_down"));
    button_row.append(&add_button);
    button_row.append(&edit_button);
    button_row.append(&delete_button);
    button_row.append(&move_up_button);
    button_row.append(&move_down_button);

    let add_current = {
        let parent = parent.clone();
        let state = Rc::clone(&state);
        let selected_index = Rc::clone(&selected_index);
        let refresh_list = Rc::clone(&refresh_list);
        let persist = Rc::clone(&persist);
        let current_directory = current_directory.clone();
        let list = list.clone();
        Rc::new(move || {
            let default_path = current_directory.clone().unwrap_or_default();
            let default_name = default_favorite_name(&default_path);
            prompt_favorite_entry(
                &parent,
                &t!("dialog.favorites_add_title"),
                default_name,
                default_path.display().to_string(),
                {
                    let state = Rc::clone(&state);
                    let selected_index = Rc::clone(&selected_index);
                    let refresh_list = Rc::clone(&refresh_list);
                    let persist = Rc::clone(&persist);
                    let list = list.clone();
                    move |entry| {
                        state.borrow_mut().push(entry);
                        let index = state.borrow().len().saturating_sub(1);
                        selected_index.set(Some(index));
                        refresh_list();
                        persist();
                        if let Some(row) = list.row_at_index(index as i32) {
                            list.select_row(Some(&row));
                            row.grab_focus();
                        } else {
                            list.grab_focus();
                        }
                    }
                },
            );
        })
    };

    {
        let add_current = Rc::clone(&add_current);
        add_button.connect_clicked(move |_| {
            add_current();
        });
    }

    {
        let parent = parent.clone();
        let state = Rc::clone(&state);
        let selected_index = Rc::clone(&selected_index);
        let refresh_list = Rc::clone(&refresh_list);
        let persist = Rc::clone(&persist);
        edit_button.connect_clicked(move |_| {
            let Some(index) = selected_index.get() else {
                show_error(
                    &parent,
                    &t!("error.favorites_unavailable"),
                    &t!("error.no_favorite_selected"),
                );
                return;
            };
            let Some(entry) = state.borrow().get(index).cloned() else {
                return;
            };
            prompt_favorite_entry(
                &parent,
                &t!("dialog.favorites_edit_title"),
                entry.name,
                entry.path.display().to_string(),
                {
                    let state = Rc::clone(&state);
                    let selected_index = Rc::clone(&selected_index);
                    let refresh_list = Rc::clone(&refresh_list);
                    let persist = Rc::clone(&persist);
                    move |entry| {
                        if let Some(slot) = state.borrow_mut().get_mut(index) {
                            *slot = entry;
                        }
                        selected_index.set(Some(index));
                        refresh_list();
                        persist();
                    }
                },
            );
        });
    }

    {
        let parent = parent.clone();
        let state = Rc::clone(&state);
        let selected_index = Rc::clone(&selected_index);
        let refresh_list = Rc::clone(&refresh_list);
        let persist = Rc::clone(&persist);
        delete_button.connect_clicked(move |_| {
            let Some(index) = selected_index.get() else {
                show_error(
                    &parent,
                    &t!("error.favorites_unavailable"),
                    &t!("error.no_favorite_selected"),
                );
                return;
            };
            let mut favorites = state.borrow_mut();
            if index < favorites.len() {
                favorites.remove(index);
            }
            let empty = favorites.is_empty();
            drop(favorites);
            selected_index.set(if empty {
                None
            } else {
                Some(index.saturating_sub(1))
            });
            refresh_list();
            persist();
        });
    }

    let move_selected = {
        let parent = parent.clone();
        let state = Rc::clone(&state);
        let selected_index = Rc::clone(&selected_index);
        let refresh_list = Rc::clone(&refresh_list);
        let persist = Rc::clone(&persist);
        let list = list.clone();
        Rc::new(move |delta: isize| {
            let Some(index) = selected_index.get() else {
                show_error(
                    &parent,
                    &t!("error.favorites_unavailable"),
                    &t!("error.no_favorite_selected"),
                );
                return;
            };
            let len = state.borrow().len();
            if len < 2 {
                return;
            }
            let Some(new_index) = index.checked_add_signed(delta) else {
                return;
            };
            if new_index >= len {
                return;
            }
            state.borrow_mut().swap(index, new_index);
            selected_index.set(Some(new_index));
            refresh_list();
            persist();
            if let Some(row) = list.row_at_index(new_index as i32) {
                list.select_row(Some(&row));
                row.grab_focus();
            }
        })
    };

    {
        let move_selected = Rc::clone(&move_selected);
        move_up_button.connect_clicked(move |_| {
            move_selected(-1);
        });
    }
    {
        let move_selected = Rc::clone(&move_selected);
        move_down_button.connect_clicked(move |_| {
            move_selected(1);
        });
    }

    let close_button = gtk::Button::with_label(&t!("common.close"));
    let go_button = gtk::Button::with_label(&t!("dialog.favorites_go"));
    go_button.add_css_class("suggested-action");
    actions.append(&close_button);
    actions.append(&go_button);
    window.set_default_widget(Some(&go_button));

    {
        let window = window.clone();
        close_button.connect_clicked(move |_| {
            window.close();
        });
    }
    {
        let go_selected = Rc::clone(&go_selected);
        go_button.connect_clicked(move |_| {
            go_selected();
        });
    }

    let initially_empty = state.borrow().is_empty();
    if initially_empty {
        present_with_focus(window.clone(), &add_button);
        // Offer adding the current folder right away when there are no favorites yet.
        let add_current = Rc::clone(&add_current);
        glib::idle_add_local_once(move || {
            add_current();
        });
    } else {
        present_with_focus(window, &list);
        let index = selected_index.get().unwrap_or(0) as i32;
        if let Some(row) = list.row_at_index(index) {
            list.select_row(Some(&row));
            row.grab_focus();
        } else {
            list.grab_focus();
        }
    }
}

fn favorite_row(entry: &FavoriteEntry) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_tooltip_text(Some(&entry.path.display().to_string()));

    let name = gtk::Label::new(Some(&entry.name));
    name.set_xalign(0.0);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    name.set_margin_top(4);
    name.set_margin_bottom(4);
    name.set_margin_start(8);
    name.set_margin_end(8);
    row.set_child(Some(&name));
    row
}

fn prompt_favorite_entry<F>(
    parent: &gtk::ApplicationWindow,
    title: &str,
    initial_name: String,
    initial_path: String,
    on_confirm: F,
) where
    F: FnOnce(FavoriteEntry) + 'static,
{
    let ModalWindow {
        window,
        content,
        actions,
    } = build_modal_window(parent, title, 480, 180);

    let title_label = gtk::Label::new(Some(title));
    title_label.set_xalign(0.0);
    title_label.add_css_class("dialog-title");
    content.append(&title_label);

    let name_label = gtk::Label::new(Some(&t!("dialog.favorites_name_prompt")));
    name_label.set_xalign(0.0);
    content.append(&name_label);
    let name_entry = gtk::Entry::new();
    name_entry.set_text(&initial_name);
    name_entry.set_hexpand(true);
    name_entry.set_activates_default(true);
    content.append(&name_entry);

    let path_label = gtk::Label::new(Some(&t!("dialog.favorites_path_prompt")));
    path_label.set_xalign(0.0);
    content.append(&path_label);
    let path_entry = gtk::Entry::new();
    path_entry.set_text(&initial_path);
    path_entry.set_hexpand(true);
    path_entry.set_activates_default(true);
    content.append(&path_entry);

    let cancel_button = gtk::Button::with_label(&t!("common.cancel"));
    let confirm_button = gtk::Button::with_label(&t!("common.save"));
    confirm_button.add_css_class("suggested-action");
    actions.append(&cancel_button);
    actions.append(&confirm_button);
    window.set_default_widget(Some(&confirm_button));

    {
        let window = window.clone();
        cancel_button.connect_clicked(move |_| {
            window.close();
        });
    }

    let callback = Rc::new(RefCell::new(Some(on_confirm)));
    {
        let window = window.clone();
        let parent = parent.clone();
        let name_entry = name_entry.clone();
        let path_entry = path_entry.clone();
        let callback = Rc::clone(&callback);
        confirm_button.connect_clicked(move |_| {
            let name = name_entry.text().trim().to_string();
            let path_text = path_entry.text().trim().to_string();
            if name.is_empty() || path_text.is_empty() {
                show_error(
                    &parent,
                    &t!("error.favorites_unavailable"),
                    &t!("error.favorite_fields_required"),
                );
                return;
            }
            let path = PathBuf::from(&path_text);
            if !path.is_dir() {
                show_error(
                    &parent,
                    &t!("error.favorites_unavailable"),
                    &t!(
                        "error.favorite_path_missing",
                        path = path.display().to_string()
                    ),
                );
                return;
            }
            if let Some(on_confirm) = callback.borrow_mut().take() {
                on_confirm(FavoriteEntry { name, path });
            }
            window.close();
        });
    }

    glib::idle_add_local_once(move || {
        window.present();
        name_entry.grab_focus();
        name_entry.select_region(0, -1);
    });
}

fn default_favorite_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.display().to_string())
}
