use std::{path::PathBuf, rc::Rc};

use gtk::{gio, glib, prelude::*};
use rust_i18n::t;
use sourceview5::{self as sourceview, prelude::*};

use crate::ui::sourceview_lang::{apply_language, apply_style_scheme};

use super::{
    support::{
        current_buffer_text, ensure_discardable, place_cursor_at_start, read_text_file,
        request_close_after_confirm, save_text_file, update_title,
    },
    EditorWidgets,
};

pub(super) fn build_editor_menu_bar() -> gtk::PopoverMenuBar {
    let menu = gio::Menu::new();
    menu.append_submenu(Some(&t!("editor.menu_file")), &{
        let file = gio::Menu::new();
        append_menu_item(&file, &t!("editor.new"), "editor.new");
        append_menu_item(&file, &t!("editor.open"), "editor.open");
        file.append_section(None, &{
            let section = gio::Menu::new();
            append_menu_item(&section, &t!("editor.save"), "editor.save");
            append_menu_item(&section, &t!("editor.save_as"), "editor.save-as");
            section
        });
        file.append_section(None, &{
            let section = gio::Menu::new();
            append_menu_item(&section, &t!("editor.exit"), "editor.exit");
            section
        });
        file
    });
    menu.append_submenu(Some(&t!("editor.menu_edit")), &{
        let edit = gio::Menu::new();
        append_menu_item(&edit, &t!("editor.undo"), "editor.undo");
        append_menu_item(&edit, &t!("editor.redo"), "editor.redo");
        edit.append_section(None, &{
            let section = gio::Menu::new();
            append_menu_item(&section, &t!("editor.cut"), "editor.cut");
            append_menu_item(&section, &t!("editor.copy"), "editor.copy");
            append_menu_item(&section, &t!("editor.paste"), "editor.paste");
            append_menu_item(&section, &t!("editor.delete"), "editor.delete");
            section
        });
        edit.append_section(None, &{
            let section = gio::Menu::new();
            append_menu_item(&section, &t!("editor.select_all"), "editor.select-all");
            append_menu_item(
                &section,
                &t!("editor.duplicate_line"),
                "editor.duplicate-line",
            );
            section
        });
        edit.append_section(None, &{
            let section = gio::Menu::new();
            append_menu_item(&section, &t!("editor.find"), "editor.find");
            append_menu_item(&section, &t!("editor.find_next"), "editor.find-next");
            append_menu_item(
                &section,
                &t!("editor.find_previous"),
                "editor.find-previous",
            );
            append_menu_item(&section, &t!("editor.replace"), "editor.replace");
            append_menu_item(&section, &t!("editor.go_to_line"), "editor.go-to-line");
            section
        });
        edit
    });

    let menubar = gtk::PopoverMenuBar::from_model(Some(&menu));
    menubar.add_css_class("app-menubar");
    menubar
}

fn append_menu_item(menu: &gio::Menu, label: &str, action: &str) {
    menu.append_item(&gio::MenuItem::new(Some(label), Some(action)));
}

pub(super) fn install_editor_actions(widgets: &Rc<EditorWidgets>) {
    let group = gio::SimpleActionGroup::new();

    add_action(&group, "new", {
        let widgets = Rc::clone(widgets);
        move |_| new_file(&widgets)
    });
    add_action(&group, "open", {
        let widgets = Rc::clone(widgets);
        move |_| open_file(&widgets)
    });
    add_action(&group, "save", {
        let widgets = Rc::clone(widgets);
        move |_| {
            save_current(&widgets, false);
        }
    });
    add_action(&group, "save-as", {
        let widgets = Rc::clone(widgets);
        move |_| {
            save_current(&widgets, true);
        }
    });
    add_action(&group, "exit", {
        let widgets = Rc::clone(widgets);
        move |_| request_close_after_confirm(&widgets)
    });
    add_action(&group, "undo", {
        let widgets = Rc::clone(widgets);
        move |_| {
            if widgets.buffer.can_undo() {
                widgets.buffer.undo();
            }
        }
    });
    add_action(&group, "redo", {
        let widgets = Rc::clone(widgets);
        move |_| {
            if widgets.buffer.can_redo() {
                widgets.buffer.redo();
            }
        }
    });
    add_action(&group, "cut", {
        let widgets = Rc::clone(widgets);
        move |_| {
            widgets
                .buffer
                .cut_clipboard(&widgets.view.clipboard(), true);
        }
    });
    add_action(&group, "copy", {
        let widgets = Rc::clone(widgets);
        move |_| {
            widgets.buffer.copy_clipboard(&widgets.view.clipboard());
        }
    });
    add_action(&group, "paste", {
        let widgets = Rc::clone(widgets);
        move |_| {
            widgets
                .buffer
                .paste_clipboard(&widgets.view.clipboard(), None, true);
        }
    });
    add_action(&group, "delete", {
        let widgets = Rc::clone(widgets);
        move |_| {
            let _ = widgets.buffer.delete_selection(true, true);
        }
    });
    add_action(&group, "select-all", {
        let widgets = Rc::clone(widgets);
        move |_| {
            let start = widgets.buffer.start_iter();
            let end = widgets.buffer.end_iter();
            widgets.buffer.select_range(&start, &end);
        }
    });
    add_action(&group, "duplicate-line", {
        let widgets = Rc::clone(widgets);
        move |_| super::support::duplicate_line_or_selection(&widgets)
    });
    add_action(&group, "find", {
        let widgets = Rc::clone(widgets);
        move |_| super::find::show_find_bar(&widgets, false)
    });
    add_action(&group, "find-next", {
        let widgets = Rc::clone(widgets);
        move |_| super::find::find_next(&widgets)
    });
    add_action(&group, "find-previous", {
        let widgets = Rc::clone(widgets);
        move |_| super::find::find_previous(&widgets)
    });
    add_action(&group, "replace", {
        let widgets = Rc::clone(widgets);
        move |_| super::find::show_find_bar(&widgets, true)
    });
    add_action(&group, "go-to-line", {
        let widgets = Rc::clone(widgets);
        move |_| super::find::go_to_line(&widgets)
    });

    widgets.window.insert_action_group("editor", Some(&group));
}

fn add_action(
    group: &gio::SimpleActionGroup,
    name: &str,
    f: impl Fn(&gio::SimpleAction) + 'static,
) {
    let action = gio::SimpleAction::new(name, None);
    action.connect_activate(move |action, _| f(action));
    group.add_action(&action);
}

pub(super) fn install_editor_shortcuts(window: &gtk::ApplicationWindow) {
    let controller = gtk::ShortcutController::new();
    controller.set_scope(gtk::ShortcutScope::Local);
    for (accel, action) in [
        ("<Control>n", "editor.new"),
        ("<Control>o", "editor.open"),
        ("<Control>s", "editor.save"),
        ("<Control><Shift>s", "editor.save-as"),
        ("<Control>w", "editor.exit"),
        ("<Control>z", "editor.undo"),
        ("<Control>y", "editor.redo"),
        ("<Control>x", "editor.cut"),
        ("<Control>c", "editor.copy"),
        ("<Control>v", "editor.paste"),
        ("Delete", "editor.delete"),
        ("<Control>a", "editor.select-all"),
        ("<Control><Shift>d", "editor.duplicate-line"),
        ("<Control>f", "editor.find"),
        ("F3", "editor.find-next"),
        ("<Shift>F3", "editor.find-previous"),
        ("<Control>h", "editor.replace"),
        ("<Control>g", "editor.go-to-line"),
    ] {
        if let (Some(trigger), Some(action)) = (
            gtk::ShortcutTrigger::parse_string(accel),
            gtk::ShortcutAction::parse_string(&format!("action({action})")),
        ) {
            controller.add_shortcut(gtk::Shortcut::new(Some(trigger), Some(action)));
        }
    }
    window.add_controller(controller);
}

fn new_file(widgets: &Rc<EditorWidgets>) {
    let widgets = Rc::clone(widgets);
    glib::MainContext::default().spawn_local(async move {
        if !ensure_discardable(&widgets).await {
            return;
        }
        widgets.buffer.begin_irreversible_action();
        widgets.buffer.set_text("");
        widgets.buffer.end_irreversible_action();
        widgets.buffer.set_language(None::<&sourceview::Language>);
        place_cursor_at_start(&widgets.buffer);
        widgets.buffer.set_modified(false);
        *widgets.path.borrow_mut() = None;
        update_title(&widgets);
        widgets.view.grab_focus();
    });
}

fn open_file(widgets: &Rc<EditorWidgets>) {
    let widgets = Rc::clone(widgets);
    glib::MainContext::default().spawn_local(async move {
        if !ensure_discardable(&widgets).await {
            return;
        }

        let dialog = gtk::FileDialog::builder()
            .title(t!("editor.open_title").into_owned())
            .modal(true)
            .build();
        if let Some(path) = widgets.path.borrow().as_ref() {
            if let Some(parent) = path.parent() {
                dialog.set_initial_folder(Some(&gio::File::for_path(parent)));
            }
        }

        let Ok(file) = dialog.open_future(Some(&widgets.window)).await else {
            return;
        };
        let Some(path) = file.path() else {
            return;
        };

        match read_text_file(&path) {
            Ok(text) => {
                widgets.buffer.begin_irreversible_action();
                widgets.buffer.set_text(&text);
                widgets.buffer.end_irreversible_action();
                apply_language(&widgets.buffer, &path);
                apply_style_scheme(&widgets.buffer, widgets.theme_preference);
                place_cursor_at_start(&widgets.buffer);
                widgets.buffer.set_modified(false);
                *widgets.path.borrow_mut() = Some(path);
                update_title(&widgets);
                widgets.view.grab_focus();
            }
            Err(error) => {
                crate::ui::dialogs::show_error(
                    &widgets.window,
                    &t!("editor.could_not_open_file"),
                    &error.to_string(),
                );
            }
        }
    });
}

fn save_current(widgets: &Rc<EditorWidgets>, force_save_as: bool) {
    let use_remote = widgets.remote.is_some() && !force_save_as;
    let known_path = widgets.path.borrow().clone().filter(|_| !force_save_as);
    if let Some(path) = known_path {
        save_to(widgets, path, use_remote);
        return;
    }

    // Only the file chooser has to be waited for.
    let widgets = Rc::clone(widgets);
    glib::MainContext::default().spawn_local(async move {
        if let Some(path) = pick_save_path(&widgets).await {
            save_to(&widgets, path, use_remote);
        }
    });
}

fn save_to(widgets: &Rc<EditorWidgets>, path: PathBuf, use_remote: bool) {
    let text = current_buffer_text(&widgets.buffer);
    match save_text_file(&path, &text) {
        Ok(()) => {
            if use_remote {
                let Some(remote) = widgets.remote.clone() else {
                    return;
                };
                let Some(upload) = widgets.remote_upload.clone() else {
                    return;
                };
                let widgets_for_callback = Rc::clone(widgets);
                let path_for_callback = path.clone();
                upload(
                    path,
                    remote,
                    Box::new(move |result| match result {
                        Ok(()) => {
                            widgets_for_callback.buffer.set_modified(false);
                            update_title(&widgets_for_callback);
                            (widgets_for_callback.on_saved)(path_for_callback);
                        }
                        Err(error) => {
                            crate::ui::dialogs::show_error(
                                &widgets_for_callback.window,
                                &t!("editor.could_not_upload_remote"),
                                &error,
                            );
                        }
                    }),
                );
            } else {
                apply_language(&widgets.buffer, &path);
                widgets.buffer.set_modified(false);
                *widgets.path.borrow_mut() = Some(path.clone());
                // Save As to a local path drops the remote binding.
                // (remote field is not mut here — keep remote for Save As as local-only write)
                update_title(widgets);
                (widgets.on_saved)(path);
            }
        }
        Err(error) => {
            crate::ui::dialogs::show_error(
                &widgets.window,
                &t!("editor.could_not_save_file"),
                &error.to_string(),
            );
        }
    }
}

async fn pick_save_path(widgets: &EditorWidgets) -> Option<PathBuf> {
    let dialog = gtk::FileDialog::builder()
        .title(t!("editor.save_title").into_owned())
        .modal(true)
        .build();
    if let Some(path) = widgets.path.borrow().as_ref() {
        dialog.set_initial_file(Some(&gio::File::for_path(path)));
    }

    let Ok(file) = dialog.save_future(Some(&widgets.window)).await else {
        return None;
    };
    file.path()
}
