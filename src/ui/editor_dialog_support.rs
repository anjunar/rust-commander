use std::{
    cell::{Cell, RefCell},
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

use anyhow::{bail, Context, Result};
use gtk::{gdk, glib, prelude::*};
use rust_i18n::t;
use sourceview5 as sourceview;

use crate::{application::ConfigStore, config::ThemePreference, ui::dsl::Ctx};

use super::{RemoteEditBinding, RemoteUploadHook};

pub(super) struct EditorWidgets {
    pub(super) window: gtk::ApplicationWindow,
    pub(super) title_label: gtk::Label,
    pub(super) view: sourceview::View,
    pub(super) buffer: sourceview::Buffer,
    pub(super) find_revealer: gtk::Revealer,
    pub(super) find_entry: gtk::Entry,
    pub(super) replace_entry: gtk::Entry,
    pub(super) replace_row: gtk::Box,
    pub(super) search_settings: sourceview::SearchSettings,
    pub(super) search_context: sourceview::SearchContext,
    pub(super) path: Rc<RefCell<Option<PathBuf>>>,
    pub(super) allow_close: Rc<Cell<bool>>,
    pub(super) on_saved: Rc<dyn Fn(PathBuf)>,
    pub(super) remote: Option<RemoteEditBinding>,
    pub(super) remote_upload: Option<RemoteUploadHook>,
    pub(super) theme_preference: ThemePreference,
}

pub(super) fn schedule_editor_focus(widgets: &Rc<EditorWidgets>) {
    // Present immediately while we may still be inside a user-input handler
    // (e.g. New File confirm). Deferred-only present often loses on Windows.
    present_editor_window(widgets);

    // Everything later holds the editor weakly: a window closed by then stays
    // closed, and none of this may keep it alive.
    let editor = Rc::downgrade(widgets);

    #[cfg(target_os = "windows")]
    {
        let editor = editor.clone();
        widgets.window.connect_map(move |_| {
            if let Some(widgets) = editor.upgrade() {
                present_editor_window(&widgets);
            }
        });
    }

    glib::idle_add_local_once(move || {
        let Some(widgets) = editor.upgrade() else {
            return;
        };
        place_cursor_at_start(&widgets.buffer);
        let mut start = widgets.buffer.start_iter();
        let _ = widgets.view.scroll_to_iter(&mut start, 0.0, true, 0.0, 0.0);
        present_editor_window(&widgets);
        #[cfg(target_os = "windows")]
        for delay_ms in [50_u64, 150, 350] {
            let editor = editor.clone();
            glib::timeout_add_local_once(Duration::from_millis(delay_ms), move || {
                if let Some(widgets) = editor.upgrade() {
                    present_editor_window(&widgets);
                }
            });
        }
    });
}

fn present_editor_window(widgets: &EditorWidgets) {
    widgets.window.present();
    #[cfg(target_os = "windows")]
    crate::platform::force_foreground_gtk_window(&widgets.window);
    let _ = widgets.view.grab_focus();
}

pub(super) async fn ensure_discardable(widgets: &EditorWidgets) -> bool {
    if !widgets.buffer.is_modified() {
        return true;
    }
    confirm_discard(&widgets.window).await
}

pub(super) fn request_close_after_confirm(widgets: &Rc<EditorWidgets>) {
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

pub(super) fn update_title(widgets: &EditorWidgets) {
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

pub(super) fn install_quiet_cursor_boundary_keys(
    view: &sourceview::View,
    buffer: &sourceview::Buffer,
) {
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

pub(super) fn place_cursor_at_start(buffer: &sourceview::Buffer) {
    let start = buffer.start_iter();
    buffer.place_cursor(&start);
}

pub(super) fn update_editor_status(buffer: &sourceview::Buffer, status_label: &gtk::Label) {
    let iter = buffer.iter_at_mark(&buffer.get_insert());
    let line = iter.line() + 1;
    let column = iter.line_offset() + 1;
    status_label.set_label(&t!("editor.status", line = line, col = column));
}

pub(super) fn read_text_file(path: &Path) -> Result<String> {
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

pub(super) fn save_text_file(path: &Path, text: &str) -> Result<()> {
    fs::write(path, text).with_context(|| format!("Could not save file {}", path.display()))
}

pub(super) fn current_buffer_text(buffer: &sourceview::Buffer) -> String {
    let start = buffer.start_iter();
    let end = buffer.end_iter();
    buffer.text(&start, &end, true).to_string()
}

pub(super) fn duplicate_line_or_selection(widgets: &EditorWidgets) {
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

/// Keeps the remembered geometry current while the window is open.
///
/// Both belong to the window's scope: a closed window reports a size of zero,
/// and it can outlive its closing because its actions hold on to it.
pub(super) fn install_editor_geometry_tracking(
    window: &Ctx<gtk::ApplicationWindow>,
    config_store: &ConfigStore,
) {
    {
        let config_store = config_store.clone();
        let handler = window.widget().connect_maximized_notify(move |window| {
            remember_editor_geometry(window, &config_store);
        });
        window.own_signal(handler);
    }

    let tracked = window.widget().clone();
    let config_store = config_store.clone();
    window.every(Duration::from_millis(250), move || {
        remember_editor_geometry(&tracked, &config_store);
    });
}

fn remember_editor_geometry(window: &gtk::ApplicationWindow, config_store: &ConfigStore) {
    let cache = config_store.cache();
    let mut app_config = cache.borrow_mut();
    app_config
        .editor
        .remember_window(window.width(), window.height(), window.is_maximized());
}

pub(super) fn persist_editor_geometry(window: &gtk::ApplicationWindow, config_store: &ConfigStore) {
    if let Err(error) = config_store.update(|app_config| {
        app_config
            .editor
            .remember_window(window.width(), window.height(), window.is_maximized());
    }) {
        eprintln!("Could not save editor window state: {error}");
    }
}
