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
        dialogs::independent_window,
        dsl::{button, container, hbox, mount, scrolled, text, tr, vbox},
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

/// Opens `path` in an editor window of its own. `on_saved` runs after each save.
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

/// Like [`edit_file`], for the local copy of a remote file that goes back on save.
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

    let search_settings = sourceview::SearchSettings::new();
    search_settings.set_wrap_around(true);
    let search_context = sourceview::SearchContext::new(&buffer, Some(&search_settings));
    search_context.set_highlight(true);

    let size = (editor_config.width.max(1), editor_config.height.max(1));
    let (window, widgets) = independent_window(parent, &title_text, size, |editor| {
        let window = editor.window();

        let mut find_entry = None;
        let mut replace_entry = None;
        let mut replace_row = None;
        let mut replace_button = None;
        let mut view = None;

        let (find_revealer, status_label) = editor.content(|| {
            let revealer = gtk::Revealer::builder()
                .transition_type(gtk::RevealerTransitionType::SlideDown)
                .reveal_child(false)
                .build();
            let find_revealer = container(revealer, |_| {
                vbox(6, |bar| {
                    bar.css("editor-find-bar");
                    hbox(6, |_| {
                        find_entry = Some(mount(gtk::Entry::new(), |entry| {
                            entry.set_placeholder_text(Some(&t!("editor.find_placeholder")));
                            entry.set_hexpand(true);
                        }));
                    });
                    replace_row = Some(hbox(6, |row| {
                        row.set_visible(false);
                        replace_entry = Some(mount(gtk::Entry::new(), |entry| {
                            entry.set_placeholder_text(Some(&t!("editor.replace_placeholder")));
                            entry.set_hexpand(true);
                        }));
                        replace_button = Some(button(tr!("editor.replace"), |_| {}));
                    }));
                });
            });

            scrolled(|scroller| {
                scroller.set_hexpand(true);
                scroller.set_vexpand(true);
                scroller.css("panel-scroller");
                view = Some(mount(sourceview::View::with_buffer(&buffer), |view| {
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
                    view.css("editor-view");
                }));
            });

            let status_label = text("", |status| status.css("editor-status"));
            (find_revealer, status_label)
        });

        let view = view.expect("editor view composed");
        let find_entry = find_entry.expect("find entry composed");
        let replace_button = replace_button.expect("replace button composed");
        install_quiet_cursor_boundary_keys(view.widget(), &buffer);

        update_editor_status(&buffer, status_label.widget());
        {
            let status_label = status_label.widget().clone();
            let handler = buffer.connect_cursor_position_notify(move |buffer| {
                update_editor_status(buffer, &status_label);
            });
            window.own_object_signal(&buffer, handler);
        }

        let widgets = Rc::new(EditorWidgets {
            window: window.widget().clone(),
            title_label: editor.title_label().clone(),
            view: view.widget().clone(),
            buffer: buffer.clone(),
            find_revealer: find_revealer.widget().clone(),
            find_entry: find_entry.widget().clone(),
            replace_entry: replace_entry
                .expect("replace entry composed")
                .widget()
                .clone(),
            replace_row: replace_row.expect("replace row composed").widget().clone(),
            search_settings: search_settings.clone(),
            search_context: search_context.clone(),
            path: Rc::new(std::cell::RefCell::new(Some(path.clone()))),
            allow_close: Rc::new(std::cell::Cell::new(false)),
            on_saved: Rc::new(on_saved),
            remote,
            remote_upload,
            theme_preference,
        });

        editor
            .header()
            .widget()
            .pack_start(&build_editor_menu_bar());
        install_editor_actions(&widgets);
        install_editor_shortcuts(window.widget());

        // Unsaved changes are confirmed before the window may close.
        {
            let widgets = Rc::clone(&widgets);
            let config_store = config_store.clone();
            window.on_close_request(move || {
                if !widgets.allow_close.get() && widgets.buffer.is_modified() {
                    request_close_after_confirm(&widgets);
                    return true;
                }
                persist_editor_geometry(&widgets.window, &config_store);
                if let Some(remote) = widgets.remote.as_ref() {
                    crate::fs::temp::remove_temp_dir(&remote.temp_dir);
                }
                false
            });
        }
        {
            let widgets = Rc::clone(&widgets);
            find_entry.on_activate(move || find_next(&widgets));
        }
        {
            let widgets = Rc::clone(&widgets);
            replace_button.on_click(move || replace_current_match(&widgets));
        }

        // Escape closes the find bar first, then the editor.
        {
            let widgets = Rc::clone(&widgets);
            let keys = gtk::EventControllerKey::new();
            keys.set_propagation_phase(gtk::PropagationPhase::Capture);
            keys.connect_key_pressed(move |_, key, _, _| {
                if key != gdk::Key::Escape {
                    return glib::Propagation::Proceed;
                }
                if widgets.find_revealer.reveals_child() {
                    hide_find_bar(&widgets);
                    widgets.view.grab_focus();
                } else {
                    request_close_after_confirm(&widgets);
                }
                glib::Propagation::Stop
            });
            window.own_controller(keys);
        }

        widgets
    });

    if editor_config.maximized {
        let window = window.clone();
        glib::idle_add_local_once(move || window.maximize());
    }
    install_editor_geometry_tracking(&window, &config_store);
    schedule_editor_focus(&widgets);

    Ok(())
}
