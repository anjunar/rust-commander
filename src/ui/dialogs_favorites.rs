use std::{
    cell::Cell,
    path::{Path, PathBuf},
    rc::Rc,
};

use gtk::{glib, prelude::*};
use rust_i18n::t;

use crate::{
    config::FavoriteEntry,
    ui::dsl::{button, entry, hbox, mount, scrolled, text, Ctx, IntoText, ListProperty, Property},
};

use super::dialogs_base::{heading, modal, paragraph, present_and_focus, show_error, Once};

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
    let model = Rc::new(FavoritesModel {
        parent: parent.clone(),
        rows: gtk::ListBox::new(),
        entries: ListProperty::new(favorites),
        selected: Cell::new(None),
        rebuilding: Cell::new(false),
        current_directory,
        on_action: Box::new(on_action),
    });

    let (window, (add_button, go_button)) = modal(
        parent,
        &t!("dialog.favorites_title"),
        (560, 420),
        |dialog| {
            let add_button = dialog.content(|| {
                heading(t!("dialog.favorites_title"));
                paragraph(t!("dialog.favorites_hint"));

                scrolled(|scroller| {
                    scroller.set_hexpand(true);
                    scroller.set_vexpand(true);
                    scroller.css("panel-scroller");
                    favorites_list(&model, dialog.window().widget());
                });

                let mut add_button = None;
                hbox(8, |buttons| {
                    buttons.set_halign(gtk::Align::Start);
                    add_button = Some(model_button(
                        t!("dialog.favorites_add"),
                        &model,
                        add_favorite,
                    ));
                    model_button(t!("dialog.favorites_edit"), &model, edit_selected);
                    model_button(t!("dialog.favorites_delete"), &model, delete_selected);
                    model_button(t!("dialog.favorites_move_up"), &model, |model| {
                        move_selected(model, -1)
                    });
                    model_button(t!("dialog.favorites_move_down"), &model, |model| {
                        move_selected(model, 1)
                    });
                });
                add_button.expect("add button composed")
            });

            let go_button = dialog.actions(|| {
                dialog.close_button(t!("common.close"));
                button(t!("dialog.favorites_go"), |button| {
                    button.css("suggested-action");
                    let model = Rc::clone(&model);
                    let window = dialog.window().widget().clone();
                    button.on_click(move || go_selected(&model, &window));
                })
            });

            (add_button, go_button)
        },
    );
    window.set_default_widget(Some(go_button.widget()));

    if model.entries.is_empty() {
        present_and_focus(&window, add_button.widget());
        // Offer adding the current folder right away when there are no favorites yet.
        glib::idle_add_local_once(move || add_favorite(&model));
    } else {
        present_and_focus(&window, &model.rows);
        model.focus_row(model.selected.get().unwrap_or(0));
    }
}

/// The favorites being edited and the row the buttons act on.
struct FavoritesModel {
    parent: gtk::ApplicationWindow,
    rows: gtk::ListBox,
    entries: ListProperty<FavoriteEntry>,
    selected: Cell<Option<usize>>,
    /// Set while the rows are replaced, so the selection changes that causes
    /// are not taken for the user's.
    rebuilding: Cell<bool>,
    current_directory: Option<PathBuf>,
    on_action: Box<dyn Fn(FavoritesAction)>,
}

impl FavoritesModel {
    /// Replaces the rows and keeps the selection on one that still exists.
    fn show_entries(&self, entries: &[FavoriteEntry]) {
        self.rebuilding.set(true);
        while let Some(row) = self.rows.row_at_index(0) {
            self.rows.remove(&row);
        }
        for entry in entries {
            self.rows.append(&favorite_row(entry));
        }

        let selected = match self.selected.get() {
            Some(index) if index < entries.len() => Some(index),
            Some(_) if !entries.is_empty() => Some(entries.len() - 1),
            None if !entries.is_empty() => Some(0),
            _ => None,
        };
        self.selected.set(selected);

        if let Some(row) = selected.and_then(|index| self.rows.row_at_index(index as i32)) {
            self.rows.select_row(Some(&row));
        }
        self.rebuilding.set(false);
    }

    /// Stores the edited list, selects a row in it and persists it.
    fn commit(&self, entries: Vec<FavoriteEntry>, selected: Option<usize>) {
        self.selected.set(selected);
        self.entries.replace_all(entries);
        (self.on_action)(FavoritesAction::Update(self.entries.get()));
    }

    fn focus_row(&self, index: usize) {
        if let Some(row) = self.rows.row_at_index(index as i32) {
            self.rows.select_row(Some(&row));
            row.grab_focus();
        } else {
            self.rows.grab_focus();
        }
    }

    /// The selected row; without one the user is told so.
    fn require_selection(&self) -> Option<usize> {
        let selected = self.selected.get();
        if selected.is_none() {
            show_error(
                &self.parent,
                &t!("error.favorites_unavailable"),
                &t!("error.no_favorite_selected"),
            );
        }
        selected
    }
}

fn favorites_list(model: &Rc<FavoritesModel>, window: &gtk::Window) -> Ctx<gtk::ListBox> {
    mount(model.rows.clone(), |list| {
        list.set_selection_mode(gtk::SelectionMode::Single);
        list.set_vexpand(true);
        list.css("favorites-list");

        {
            let shown = Rc::clone(model);
            list.add_disposable(
                model
                    .entries
                    .observe(move |entries| shown.show_entries(entries)),
            );
        }
        {
            let model = Rc::clone(model);
            let handler = list.connect_row_selected(move |_, row| {
                if !model.rebuilding.get() {
                    model.selected.set(row.map(|row| row.index() as usize));
                }
            });
            list.own_signal(handler);
        }
        {
            let model = Rc::clone(model);
            let window = window.clone();
            let handler = list.connect_row_activated(move |_, _| go_selected(&model, &window));
            list.own_signal(handler);
        }
    })
}

fn model_button(
    label: impl IntoText,
    model: &Rc<FavoritesModel>,
    action: impl Fn(&Rc<FavoritesModel>) + 'static,
) -> Ctx<gtk::Button> {
    let model = Rc::clone(model);
    button(label, |button| button.on_click(move || action(&model)))
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

fn go_selected(model: &FavoritesModel, window: &gtk::Window) {
    let Some(index) = model.require_selection() else {
        return;
    };
    let Some(entry) = model.entries.item(index) else {
        return;
    };
    if !entry.path.is_dir() {
        show_missing_path(&model.parent, &entry.path);
        return;
    }
    (model.on_action)(FavoritesAction::Go(entry.path));
    window.close();
}

fn add_favorite(model: &Rc<FavoritesModel>) {
    let default_path = model.current_directory.clone().unwrap_or_default();
    let default_name = default_favorite_name(&default_path);
    let confirmed = Rc::clone(model);
    prompt_favorite_entry(
        &model.parent,
        &t!("dialog.favorites_add_title"),
        default_name,
        default_path.display().to_string(),
        move |entry| {
            let mut entries = confirmed.entries.get();
            entries.push(entry);
            let index = entries.len() - 1;
            confirmed.commit(entries, Some(index));
            confirmed.focus_row(index);
        },
    );
}

fn edit_selected(model: &Rc<FavoritesModel>) {
    let Some(index) = model.require_selection() else {
        return;
    };
    let Some(entry) = model.entries.item(index) else {
        return;
    };
    let confirmed = Rc::clone(model);
    prompt_favorite_entry(
        &model.parent,
        &t!("dialog.favorites_edit_title"),
        entry.name,
        entry.path.display().to_string(),
        move |entry| {
            let mut entries = confirmed.entries.get();
            if let Some(slot) = entries.get_mut(index) {
                *slot = entry;
            }
            confirmed.commit(entries, Some(index));
        },
    );
}

fn delete_selected(model: &Rc<FavoritesModel>) {
    let Some(index) = model.require_selection() else {
        return;
    };
    let mut entries = model.entries.get();
    if index < entries.len() {
        entries.remove(index);
    }
    let selected = (!entries.is_empty()).then(|| index.saturating_sub(1));
    model.commit(entries, selected);
}

fn move_selected(model: &Rc<FavoritesModel>, delta: isize) {
    let Some(index) = model.require_selection() else {
        return;
    };
    let mut entries = model.entries.get();
    if entries.len() < 2 {
        return;
    }
    let Some(new_index) = index.checked_add_signed(delta) else {
        return;
    };
    if new_index >= entries.len() {
        return;
    }
    entries.swap(index, new_index);
    model.commit(entries, Some(new_index));
    model.focus_row(new_index);
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
    let name = Property::new(initial_name);
    let path = Property::new(initial_path);
    let on_confirm = Once::new(on_confirm);

    let (window, (name_entry, confirm)) = modal(parent, title, (480, 180), |dialog| {
        let name_entry = dialog.content(|| {
            heading(title);
            let name_entry = favorite_field(t!("dialog.favorites_name_prompt"), &name);
            favorite_field(t!("dialog.favorites_path_prompt"), &path);
            name_entry
        });
        let confirm = dialog.actions(|| {
            dialog.close_button(t!("common.cancel"));
            button(t!("common.save"), |button| {
                button.css("suggested-action");
                let parent = parent.clone();
                let window = dialog.window().widget().clone();
                let name = name.clone();
                let path = path.clone();
                let on_confirm = on_confirm.clone();
                button.on_click(move || {
                    let Some(entry) = validated_entry(&parent, &name.get(), &path.get()) else {
                        return;
                    };
                    on_confirm.call(entry);
                    window.close();
                });
            })
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

fn favorite_field(label: impl IntoText, value: &Property<String>) -> Ctx<gtk::Entry> {
    text(label, |_| {});
    entry(value, |entry| {
        entry.set_hexpand(true);
        entry.set_activates_default(true);
    })
}

/// The favorite the prompt describes; what is wrong with it is shown otherwise.
fn validated_entry(
    parent: &gtk::ApplicationWindow,
    name: &str,
    path: &str,
) -> Option<FavoriteEntry> {
    let name = name.trim();
    let path = path.trim();
    if name.is_empty() || path.is_empty() {
        show_error(
            parent,
            &t!("error.favorites_unavailable"),
            &t!("error.favorite_fields_required"),
        );
        return None;
    }
    let path = PathBuf::from(path);
    if !path.is_dir() {
        show_missing_path(parent, &path);
        return None;
    }
    Some(FavoriteEntry {
        name: name.to_string(),
        path,
    })
}

fn show_missing_path(parent: &gtk::ApplicationWindow, path: &Path) {
    show_error(
        parent,
        &t!("error.favorites_unavailable"),
        &t!(
            "error.favorite_path_missing",
            path = path.display().to_string()
        ),
    );
}

fn default_favorite_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.display().to_string())
}
