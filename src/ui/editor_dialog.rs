use std::{
    cell::{Cell, RefCell},
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

use anyhow::{bail, Context, Result};
use gtk::{gdk, gio, glib, prelude::*};
use rust_i18n::t;
use sourceview5::{self as sourceview, prelude::*};

use crate::{
    application::ConfigStore,
    config::ThemePreference,
    platform::assets::asset_path,
    remote::RemoteSession,
    ui::{dialogs::build_independent_window, theme},
};

/// Binding that uploads the temp local file back to SFTP on Save.
#[derive(Clone)]
pub struct RemoteEditBinding {
    pub session: RemoteSession,
    pub remote_path: String,
    pub temp_dir: PathBuf,
}

/// Called after a successful local write when editing a remote file.
/// The callback argument reports upload success/failure.
pub type RemoteUploadHook =
    Rc<dyn Fn(PathBuf, RemoteEditBinding, Box<dyn FnOnce(Result<(), String>)>)>;

struct EditorWidgets {
    window: gtk::ApplicationWindow,
    title_label: gtk::Label,
    view: sourceview::View,
    buffer: sourceview::Buffer,
    find_revealer: gtk::Revealer,
    find_entry: gtk::Entry,
    replace_entry: gtk::Entry,
    replace_row: gtk::Box,
    search_settings: sourceview::SearchSettings,
    search_context: sourceview::SearchContext,
    path: Rc<RefCell<Option<PathBuf>>>,
    allow_close: Rc<Cell<bool>>,
    on_saved: Rc<dyn Fn(PathBuf)>,
    remote: Option<RemoteEditBinding>,
    remote_upload: Option<RemoteUploadHook>,
    theme_preference: ThemePreference,
}

pub fn edit_file<F>(
    parent: &gtk::ApplicationWindow,
    path: PathBuf,
    theme_preference: ThemePreference,
    config_store: ConfigStore,
    on_saved: F,
) -> Result<()>
where
    F: Fn(PathBuf) + 'static,
{
    edit_file_inner(
        parent,
        path,
        theme_preference,
        config_store,
        None,
        None,
        on_saved,
    )
}

pub fn edit_remote_file<F>(
    parent: &gtk::ApplicationWindow,
    local_path: PathBuf,
    remote: RemoteEditBinding,
    remote_upload: RemoteUploadHook,
    theme_preference: ThemePreference,
    config_store: ConfigStore,
    on_saved: F,
) -> Result<()>
where
    F: Fn(PathBuf) + 'static,
{
    edit_file_inner(
        parent,
        local_path,
        theme_preference,
        config_store,
        Some(remote),
        Some(remote_upload),
        on_saved,
    )
}

fn edit_file_inner<F>(
    parent: &gtk::ApplicationWindow,
    path: PathBuf,
    theme_preference: ThemePreference,
    config_store: ConfigStore,
    remote: Option<RemoteEditBinding>,
    remote_upload: Option<RemoteUploadHook>,
    on_saved: F,
) -> Result<()>
where
    F: Fn(PathBuf) + 'static,
{
    let initial_text = read_text_file(&path)?;
    let editor_config = config_store.snapshot().editor;
    let title_text = remote
        .as_ref()
        .map(|binding| binding.remote_path.clone())
        .unwrap_or_else(|| path.display().to_string());
    let independent = build_independent_window(
        parent,
        &title_text,
        editor_config.width.max(1),
        editor_config.height.max(1),
    );
    let window = independent.window;
    let header = independent.header;
    let title_label = independent.title_label;
    let content = independent.content;
    independent.actions.set_visible(false);

    if editor_config.maximized {
        let window = window.clone();
        glib::idle_add_local_once(move || {
            window.maximize();
        });
    }

    install_editor_geometry_tracking(&window, &config_store);

    let buffer = sourceview::Buffer::new(None);
    buffer.set_enable_undo(true);
    buffer.begin_irreversible_action();
    buffer.set_text(&initial_text);
    buffer.end_irreversible_action();
    buffer.set_highlight_syntax(true);
    place_cursor_at_start(&buffer);
    buffer.set_modified(false);
    apply_language(&buffer, &path);
    apply_style_scheme(&buffer, theme_preference);

    let view = sourceview::View::with_buffer(&buffer);
    view.set_hexpand(true);
    view.set_vexpand(true);
    view.set_monospace(true);
    view.set_show_line_numbers(true);
    view.set_highlight_current_line(true);
    view.set_auto_indent(true);
    view.set_insert_spaces_instead_of_tabs(true);
    view.set_indent_width(4);
    view.set_tab_width(4);
    view.set_show_right_margin(true);
    view.set_right_margin_position(100);
    view.add_css_class("editor-view");
    install_quiet_cursor_boundary_keys(&view, &buffer);

    let search_settings = sourceview::SearchSettings::new();
    search_settings.set_wrap_around(true);
    let search_context = sourceview::SearchContext::new(&buffer, Some(&search_settings));
    search_context.set_highlight(true);

    let find_bar = gtk::Box::new(gtk::Orientation::Vertical, 6);
    find_bar.add_css_class("editor-find-bar");

    let find_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let find_entry = gtk::Entry::new();
    find_entry.set_placeholder_text(Some(&t!("editor.find_placeholder")));
    find_entry.set_hexpand(true);
    find_row.append(&find_entry);
    find_bar.append(&find_row);

    let replace_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let replace_entry = gtk::Entry::new();
    replace_entry.set_placeholder_text(Some(&t!("editor.replace_placeholder")));
    replace_entry.set_hexpand(true);
    let replace_button = gtk::Button::with_label(&t!("editor.replace"));
    replace_row.append(&replace_entry);
    replace_row.append(&replace_button);
    replace_row.set_visible(false);
    find_bar.append(&replace_row);

    let find_revealer = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideDown)
        .child(&find_bar)
        .reveal_child(false)
        .build();
    content.append(&find_revealer);

    let scrolled = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&view)
        .build();
    scrolled.add_css_class("panel-scroller");
    content.append(&scrolled);

    let status_label = gtk::Label::new(None);
    status_label.set_xalign(0.0);
    status_label.add_css_class("editor-status");
    content.append(&status_label);
    update_editor_status(&buffer, &status_label);

    {
        let status_label = status_label.clone();
        buffer.connect_cursor_position_notify(move |buffer| {
            update_editor_status(buffer, &status_label);
        });
    }

    let widgets = Rc::new(EditorWidgets {
        window: window.clone(),
        title_label,
        view: view.clone(),
        buffer: buffer.clone(),
        find_revealer: find_revealer.clone(),
        find_entry: find_entry.clone(),
        replace_entry: replace_entry.clone(),
        replace_row: replace_row.clone(),
        search_settings: search_settings.clone(),
        search_context: search_context.clone(),
        path: Rc::new(RefCell::new(Some(path))),
        allow_close: Rc::new(Cell::new(false)),
        on_saved: Rc::new(on_saved),
        remote,
        remote_upload,
        theme_preference,
    });

    header.pack_start(&build_editor_menu_bar());
    install_editor_actions(&widgets);
    install_editor_shortcuts(&widgets.window);

    {
        let widgets = Rc::clone(&widgets);
        let config_store = config_store.clone();
        let window = widgets.window.clone();
        window.connect_close_request(move |window| {
            if widgets.allow_close.get() || !widgets.buffer.is_modified() {
                persist_editor_geometry(window, &config_store);
                if let Some(remote) = widgets.remote.as_ref() {
                    crate::fs::temp::remove_temp_dir(&remote.temp_dir);
                }
                return glib::Propagation::Proceed;
            }

            request_close_after_confirm(&widgets);
            glib::Propagation::Stop
        });
    }

    {
        let widgets = Rc::clone(&widgets);
        find_entry.connect_activate(move |_| {
            find_next(&widgets);
        });
    }

    {
        let widgets = Rc::clone(&widgets);
        replace_button.connect_clicked(move |_| {
            replace_current_match(&widgets);
        });
    }

    {
        let widgets = Rc::clone(&widgets);
        let window = widgets.window.clone();
        let key_controller = gtk::EventControllerKey::new();
        key_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        key_controller.connect_key_pressed(move |_, key, _, _| {
            if key != gdk::Key::Escape {
                return glib::Propagation::Proceed;
            }
            if widgets.find_revealer.reveals_child() {
                hide_find_bar(&widgets);
                widgets.view.grab_focus();
                return glib::Propagation::Stop;
            }
            request_close_after_confirm(&widgets);
            glib::Propagation::Stop
        });
        window.add_controller(key_controller);
    }

    glib::idle_add_local_once({
        let widgets = Rc::clone(&widgets);
        move || {
            place_cursor_at_start(&widgets.buffer);
            let mut start = widgets.buffer.start_iter();
            let _ = widgets.view.scroll_to_iter(&mut start, 0.0, true, 0.0, 0.0);
            widgets.window.present();
            widgets.view.grab_focus();
        }
    });

    Ok(())
}

fn build_editor_menu_bar() -> gtk::PopoverMenuBar {
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
            append_menu_item(&section, &t!("editor.duplicate_line"), "editor.duplicate-line");
            section
        });
        edit.append_section(None, &{
            let section = gio::Menu::new();
            append_menu_item(&section, &t!("editor.find"), "editor.find");
            append_menu_item(&section, &t!("editor.find_next"), "editor.find-next");
            append_menu_item(&section, &t!("editor.find_previous"), "editor.find-previous");
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

fn install_editor_actions(widgets: &Rc<EditorWidgets>) {
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
        move |_| duplicate_line_or_selection(&widgets)
    });
    add_action(&group, "find", {
        let widgets = Rc::clone(widgets);
        move |_| show_find_bar(&widgets, false)
    });
    add_action(&group, "find-next", {
        let widgets = Rc::clone(widgets);
        move |_| find_next(&widgets)
    });
    add_action(&group, "find-previous", {
        let widgets = Rc::clone(widgets);
        move |_| find_previous(&widgets)
    });
    add_action(&group, "replace", {
        let widgets = Rc::clone(widgets);
        move |_| show_find_bar(&widgets, true)
    });
    add_action(&group, "go-to-line", {
        let widgets = Rc::clone(widgets);
        move |_| go_to_line(&widgets)
    });

    widgets
        .window
        .insert_action_group("editor", Some(&group));
}

fn add_action(group: &gio::SimpleActionGroup, name: &str, f: impl Fn(&gio::SimpleAction) + 'static) {
    let action = gio::SimpleAction::new(name, None);
    action.connect_activate(move |action, _| f(action));
    group.add_action(&action);
}

fn install_editor_shortcuts(window: &gtk::ApplicationWindow) {
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
    let widgets = Rc::clone(widgets);
    glib::MainContext::default().spawn_local(async move {
        let use_remote = widgets.remote.is_some() && !force_save_as;
        let target = if force_save_as || widgets.path.borrow().is_none() {
            pick_save_path(&widgets).await
        } else {
            widgets.path.borrow().clone()
        };

        let Some(path) = target else {
            return;
        };

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
                    let widgets_for_callback = Rc::clone(&widgets);
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
                    update_title(&widgets);
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
    });
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

fn show_find_bar(widgets: &EditorWidgets, with_replace: bool) {
    widgets.replace_row.set_visible(with_replace);
    widgets.find_revealer.set_reveal_child(true);
    if let Some((start, end)) = widgets.buffer.selection_bounds() {
        if start.line() == end.line() && start.offset() != end.offset() {
            let selected = widgets.buffer.text(&start, &end, false);
            if !selected.is_empty() {
                widgets.find_entry.set_text(&selected);
            }
        }
    }
    widgets.find_entry.grab_focus();
    widgets.find_entry.select_region(0, -1);
}

fn hide_find_bar(widgets: &EditorWidgets) {
    widgets.find_revealer.set_reveal_child(false);
    widgets.replace_row.set_visible(false);
    widgets.search_settings.set_search_text(None::<&str>);
}

fn sync_search_text(widgets: &EditorWidgets) {
    let text = widgets.find_entry.text();
    if text.is_empty() {
        widgets.search_settings.set_search_text(None::<&str>);
    } else {
        widgets.search_settings.set_search_text(Some(text.as_str()));
    }
}

fn find_next(widgets: &EditorWidgets) {
    if !widgets.find_revealer.reveals_child() {
        show_find_bar(widgets, widgets.replace_row.is_visible());
        return;
    }
    sync_search_text(widgets);
    if widgets.find_entry.text().is_empty() {
        return;
    }
    let mut iter = widgets.buffer.iter_at_mark(&widgets.buffer.get_insert());
    if widgets.buffer.selection_bounds().is_some() {
        iter.forward_char();
    }
    if let Some((start, end, _)) = widgets.search_context.forward(&iter) {
        widgets.buffer.select_range(&start, &end);
        let mut start = start;
        let _ = widgets.view.scroll_to_iter(&mut start, 0.1, false, 0.0, 0.0);
        let _ = end;
    }
}

fn find_previous(widgets: &EditorWidgets) {
    if !widgets.find_revealer.reveals_child() {
        show_find_bar(widgets, widgets.replace_row.is_visible());
        return;
    }
    sync_search_text(widgets);
    if widgets.find_entry.text().is_empty() {
        return;
    }
    let iter = widgets.buffer.iter_at_mark(&widgets.buffer.get_insert());
    if let Some((start, end, _)) = widgets.search_context.backward(&iter) {
        widgets.buffer.select_range(&start, &end);
        let mut start = start;
        let _ = widgets.view.scroll_to_iter(&mut start, 0.1, false, 0.0, 0.0);
        let _ = end;
    }
}

fn replace_current_match(widgets: &EditorWidgets) {
    sync_search_text(widgets);
    let replace = widgets.replace_entry.text();
    if let Some((mut start, mut end)) = widgets.buffer.selection_bounds() {
        if widgets
            .search_context
            .replace(&mut start, &mut end, replace.as_str())
            .is_ok()
        {
            find_next(widgets);
            return;
        }
    }
    find_next(widgets);
}

fn go_to_line(widgets: &Rc<EditorWidgets>) {
    let dialog = gtk::Window::builder()
        .transient_for(&widgets.window)
        .modal(true)
        .title(t!("editor.go_to_line_title").into_owned())
        .default_width(320)
        .build();
    if let Some(app) = widgets.window.application() {
        dialog.set_application(Some(&app));
    }

    let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);

    let label = gtk::Label::new(Some(&t!("editor.go_to_line_prompt")));
    label.set_xalign(0.0);
    root.append(&label);

    let entry = gtk::Entry::new();
    entry.set_input_purpose(gtk::InputPurpose::Digits);
    root.append(&entry);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label(&t!("common.cancel"));
    let ok = gtk::Button::with_label(&t!("common.ok"));
    ok.add_css_class("suggested-action");
    buttons.append(&cancel);
    buttons.append(&ok);
    root.append(&buttons);
    dialog.set_child(Some(&root));
    dialog.set_default_widget(Some(&ok));

    let apply_goto = Rc::new({
        let widgets = Rc::clone(widgets);
        let entry = entry.clone();
        let dialog = dialog.clone();
        move || {
            let Ok(line) = entry.text().parse::<i32>() else {
                return;
            };
            let total_lines = widgets.buffer.line_count().max(1);
            let target = line.clamp(1, total_lines) - 1;
            if let Some(mut iter) = widgets.buffer.iter_at_line(target) {
                widgets.buffer.place_cursor(&iter);
                let _ = widgets.view.scroll_to_iter(&mut iter, 0.2, true, 0.0, 0.0);
                widgets.view.grab_focus();
            }
            dialog.close();
        }
    });

    {
        let dialog = dialog.clone();
        cancel.connect_clicked(move |_| {
            dialog.close();
        });
    }
    {
        let apply_goto = Rc::clone(&apply_goto);
        ok.connect_clicked(move |_| apply_goto());
    }
    {
        entry.connect_activate(move |_| apply_goto());
    }

    dialog.present();
    entry.grab_focus();
}

async fn ensure_discardable(widgets: &EditorWidgets) -> bool {
    if !widgets.buffer.is_modified() {
        return true;
    }
    confirm_discard(&widgets.window).await
}

fn request_close_after_confirm(widgets: &Rc<EditorWidgets>) {
    if !widgets.buffer.is_modified() {
        widgets.allow_close.set(true);
        widgets.window.close();
        return;
    }

    let widgets = Rc::clone(widgets);
    glib::MainContext::default().spawn_local(async move {
        if confirm_discard(&widgets.window).await {
            widgets.allow_close.set(true);
            widgets.window.close();
        }
    });
}

fn update_title(widgets: &EditorWidgets) {
    let title = if let Some(remote) = widgets.remote.as_ref() {
        remote.remote_path.clone()
    } else {
        widgets
            .path
            .borrow()
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| t!("editor.untitled").into_owned())
    };
    widgets.window.set_title(Some(&title));
    widgets.title_label.set_label(&title);
    widgets.title_label.set_tooltip_text(Some(&title));
}

fn install_quiet_cursor_boundary_keys(view: &sourceview::View, buffer: &sourceview::Buffer) {
    let controller = gtk::EventControllerKey::new();
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    let buffer = buffer.clone();
    controller.connect_key_pressed(move |_, key, _, modifiers| {
        let iter = buffer.iter_at_mark(&buffer.get_insert());
        let control = modifiers.contains(gdk::ModifierType::CONTROL_MASK);
        let swallow = if key == gdk::Key::Down || key == gdk::Key::KP_Down {
            iter.line() >= buffer.line_count().max(1) - 1
        } else if key == gdk::Key::Up || key == gdk::Key::KP_Up {
            iter.line() <= 0
        } else if key == gdk::Key::Left || key == gdk::Key::KP_Left {
            iter.is_start()
        } else if key == gdk::Key::Right || key == gdk::Key::KP_Right {
            iter.is_end()
        } else if key == gdk::Key::Home || key == gdk::Key::KP_Home {
            if control {
                iter.is_start()
            } else {
                iter.starts_line()
            }
        } else if key == gdk::Key::End || key == gdk::Key::KP_End {
            if control {
                iter.is_end()
            } else {
                iter.ends_line()
            }
        } else if key == gdk::Key::Page_Down || key == gdk::Key::KP_Page_Down {
            iter.line() >= buffer.line_count().max(1) - 1
        } else if key == gdk::Key::Page_Up || key == gdk::Key::KP_Page_Up {
            iter.line() <= 0
        } else {
            false
        };

        if swallow {
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    view.add_controller(controller);
}

fn place_cursor_at_start(buffer: &sourceview::Buffer) {
    let start = buffer.start_iter();
    buffer.place_cursor(&start);
}

fn update_editor_status(buffer: &sourceview::Buffer, status_label: &gtk::Label) {
    let iter = buffer.iter_at_mark(&buffer.get_insert());
    let line = iter.line() + 1;
    let column = iter.line_offset() + 1;
    status_label.set_label(&t!("editor.status", line = line, col = column));
}

fn read_text_file(path: &Path) -> Result<String> {
    let bytes =
        fs::read(path).with_context(|| format!("Could not read file {}", path.display()))?;

    if bytes.contains(&0) {
        bail!("{}", t!("editor.binary_file_not_editable"));
    }

    String::from_utf8(bytes).with_context(|| {
        t!(
            "editor.invalid_utf8_for_edit",
            path = path.display().to_string()
        )
        .into_owned()
    })
}

fn save_text_file(path: &Path, text: &str) -> Result<()> {
    fs::write(path, text).with_context(|| format!("Could not save file {}", path.display()))
}

fn current_buffer_text(buffer: &sourceview::Buffer) -> String {
    let start = buffer.start_iter();
    let end = buffer.end_iter();
    buffer.text(&start, &end, true).to_string()
}

fn apply_language(buffer: &sourceview::Buffer, path: &Path) {
    ensure_language_search_paths();
    let manager = sourceview::LanguageManager::default();
    let language = guess_editor_language(&manager, path);
    buffer.set_language(language.as_ref());
}

fn ensure_language_search_paths() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let specs_dir = asset_path("assets/gtksourceview-5/language-specs");
        if !specs_dir.is_dir() {
            return;
        }
        let manager = sourceview::LanguageManager::default();
        let custom = specs_dir.to_string_lossy().into_owned();
        let mut paths: Vec<String> = manager
            .search_path()
            .into_iter()
            .map(|path| path.to_string())
            .collect();
        if paths.iter().any(|path| path == &custom) {
            return;
        }
        paths.insert(0, custom);
        let path_refs: Vec<&str> = paths.iter().map(String::as_str).collect();
        manager.set_search_path(&path_refs);
    });
}

fn guess_editor_language(
    manager: &sourceview::LanguageManager,
    path: &Path,
) -> Option<sourceview::Language> {
    if is_nginx_path(path) || looks_like_nginx_conf(path) {
        if let Some(language) = manager.language("nginx") {
            return Some(language);
        }
    }
    manager.guess_language(Some(path), None::<&str>)
}

fn is_nginx_path(path: &Path) -> bool {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if file_name == "nginx.conf" || file_name.ends_with(".nginx") {
        return true;
    }

    path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|part| part.eq_ignore_ascii_case("nginx"))
    })
}

fn looks_like_nginx_conf(path: &Path) -> bool {
    let is_conf = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("conf"));
    if !is_conf {
        return false;
    }

    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    let sample = String::from_utf8_lossy(&bytes[..bytes.len().min(16 * 1024)]);
    const MARKERS: &[&str] = &[
        "worker_processes",
        "server_name",
        "proxy_pass",
        "fastcgi_pass",
        "listen ",
        "location ",
        "upstream ",
        "http {",
        "events {",
    ];
    MARKERS
        .iter()
        .filter(|marker| sample.contains(*marker))
        .count()
        >= 2
}

fn duplicate_line_or_selection(widgets: &EditorWidgets) {
    let buffer = &widgets.buffer;
    buffer.begin_user_action();

    let (mut range_start, mut range_end) = if let Some((start, end)) = buffer.selection_bounds() {
        (start, end)
    } else {
        let cursor = buffer.iter_at_mark(&buffer.get_insert());
        (cursor, cursor)
    };

    if !range_start.starts_line() {
        range_start.set_line_offset(0);
    }
    if range_end.starts_line() && range_end != range_start {
        // Selection ends at the start of the next line — keep that boundary.
    } else if !range_end.ends_line() {
        range_end.forward_to_line_end();
    }

    let at_buffer_end = range_end.is_end();
    let text = if at_buffer_end {
        format!("\n{}", buffer.text(&range_start, &range_end, true))
    } else {
        let mut end_with_newline = range_end;
        end_with_newline.forward_char();
        buffer
            .text(&range_start, &end_with_newline, true)
            .to_string()
    };

    let mut insert_at = range_end;
    if !at_buffer_end {
        insert_at.forward_char();
    }
    buffer.insert(&mut insert_at, &text);
    buffer.end_user_action();
}

fn apply_style_scheme(buffer: &sourceview::Buffer, theme_preference: ThemePreference) {
    let manager = sourceview::StyleSchemeManager::default();
    let scheme = theme::sourceview_style_scheme_ids(theme_preference)
        .iter()
        .find_map(|scheme_id| manager.scheme(scheme_id));
    buffer.set_style_scheme(scheme.as_ref());
}

async fn confirm_discard(parent: &impl IsA<gtk::Window>) -> bool {
    let dialog = gtk::AlertDialog::builder()
        .modal(true)
        .message(t!("editor.discard_changes_title").into_owned())
        .detail(t!("editor.discard_changes_detail").into_owned())
        .buttons([
            t!("editor.keep_editing").into_owned(),
            t!("editor.discard").into_owned(),
        ])
        .cancel_button(0)
        .default_button(0)
        .build();

    matches!(dialog.choose_future(Some(parent)).await, Ok(1))
}

fn install_editor_geometry_tracking(window: &gtk::ApplicationWindow, config_store: &ConfigStore) {
    {
        let config_store = config_store.clone();
        window.connect_maximized_notify(move |window| {
            remember_editor_geometry(window, &config_store);
        });
    }

    let window_weak = window.downgrade();
    let config_store = config_store.clone();
    glib::timeout_add_local(Duration::from_millis(250), move || {
        let Some(window) = window_weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        remember_editor_geometry(&window, &config_store);
        glib::ControlFlow::Continue
    });
}

fn remember_editor_geometry(window: &gtk::ApplicationWindow, config_store: &ConfigStore) {
    let cache = config_store.cache();
    let mut app_config = cache.borrow_mut();
    app_config.editor.maximized = window.is_maximized();
    if !app_config.editor.maximized {
        app_config.editor.width = window.width().max(1);
        app_config.editor.height = window.height().max(1);
    }
}

fn persist_editor_geometry(window: &gtk::ApplicationWindow, config_store: &ConfigStore) {
    if let Err(error) = config_store.update(|app_config| {
        app_config.editor.maximized = window.is_maximized();
        if !app_config.editor.maximized {
            app_config.editor.width = window.width().max(1);
            app_config.editor.height = window.height().max(1);
        }
    }) {
        eprintln!("Could not save editor window state: {error}");
    }
}
