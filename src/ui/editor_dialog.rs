use std::{path::PathBuf, rc::Rc};

use anyhow::Result;
use gtk::{gdk, glib, prelude::*};
use rust_i18n::t;
use sourceview5::{self as sourceview, prelude::*};

use crate::{
    application::ConfigStore,
    config::ThemePreference,
    remote::RemoteSession,
    ui::{
        dialogs::build_independent_window,
        sourceview_lang::{apply_language, apply_style_scheme},
    },
};

#[path = "editor_dialog_actions.rs"]
mod actions;
#[path = "editor_dialog_find.rs"]
mod find;
#[path = "editor_dialog_support.rs"]
mod support;

use actions::{build_editor_menu_bar, install_editor_actions, install_editor_shortcuts};
use find::{find_next, hide_find_bar, replace_current_match};
use support::{
    install_editor_geometry_tracking, install_quiet_cursor_boundary_keys, persist_editor_geometry,
    place_cursor_at_start, read_text_file, request_close_after_confirm, schedule_editor_focus,
    update_editor_status, EditorWidgets,
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
        path: Rc::new(std::cell::RefCell::new(Some(path))),
        allow_close: Rc::new(std::cell::Cell::new(false)),
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

    schedule_editor_focus(&widgets);

    Ok(())
}
