use std::{
    cell::Cell,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
};

use anyhow::{bail, Context, Result};
use gtk::{glib, prelude::*};
use rust_i18n::t;
use sourceview5::{self as sourceview, prelude::*};

use crate::ui::{
    dialogs::{self, modal, present_and_focus},
    dsl::{button, mount, scrolled, text, tr},
};

/// Opens `path` in a modal UTF-8 editor. `on_saved` runs after a successful save.
pub fn edit_file<F>(parent: &gtk::ApplicationWindow, path: PathBuf, on_saved: F) -> Result<()>
where
    F: Fn(PathBuf) + 'static,
{
    let initial_text = read_text_file(&path)?;

    let buffer = sourceview::Buffer::new(None);
    buffer.set_text(&initial_text);
    buffer.set_highlight_syntax(true);
    buffer.set_modified(false);
    apply_language(&buffer, &path);
    apply_style_scheme(&buffer);

    let allow_close = Rc::new(Cell::new(false));
    let on_saved = Rc::new(on_saved);
    let title = t!("editor.edit_title", file = file_label(&path)).into_owned();

    let (window, view) = modal(parent, &title, (980, 720), |editor| {
        let window = editor.window().widget().clone();

        // Unsaved changes are confirmed before the window may close.
        {
            let window = window.clone();
            let parent = parent.clone();
            let buffer = buffer.clone();
            let allow_close = Rc::clone(&allow_close);
            editor.window().on_close_request(move || {
                if allow_close.get() || !buffer.is_modified() {
                    return false;
                }
                request_close_after_confirm(&window, &parent, &buffer, &allow_close);
                true
            });
        }

        let view = editor.content(|| {
            text(path.display().to_string(), |label| label.css("path-label"));
            let mut view = None;
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
            text(tr!("editor.utf8_text_file"), |status| {
                status.css("editor-status")
            });
            view.expect("editor view composed")
        });

        let save = editor.actions(|| {
            button(tr!("common.cancel"), |cancel| {
                let window = window.clone();
                let parent = parent.clone();
                let buffer = buffer.clone();
                let allow_close = Rc::clone(&allow_close);
                cancel.on_click(move || {
                    request_close_after_confirm(&window, &parent, &buffer, &allow_close)
                });
            });
            button(tr!("common.save"), |save| {
                save.css("suggested-action");
                let window = window.clone();
                let parent = parent.clone();
                let buffer = buffer.clone();
                let path = path.clone();
                let on_saved = Rc::clone(&on_saved);
                save.on_click(
                    move || match save_text_file(&path, &current_buffer_text(&buffer)) {
                        Ok(()) => {
                            buffer.set_modified(false);
                            on_saved(path.clone());
                            window.close();
                        }
                        Err(error) => {
                            dialogs::show_error(&parent, "Could not save file", &error.to_string());
                        }
                    },
                );
            })
        });
        window.set_default_widget(Some(save.widget()));

        view
    });

    present_and_focus(&window, view.widget());
    Ok(())
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
    let manager = sourceview::LanguageManager::default();
    let language = manager.guess_language(Some(path), None::<&str>);
    buffer.set_language(language.as_ref());
}

fn apply_style_scheme(buffer: &sourceview::Buffer) {
    let manager = sourceview::StyleSchemeManager::default();
    let scheme = ["Adwaita-dark", "classic-dark", "Adwaita"]
        .into_iter()
        .find_map(|scheme_id| manager.scheme(scheme_id));
    buffer.set_style_scheme(scheme.as_ref());
}

async fn confirm_discard(parent: &gtk::ApplicationWindow) -> bool {
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

fn request_close_after_confirm(
    window: &gtk::Window,
    parent: &gtk::ApplicationWindow,
    buffer: &sourceview::Buffer,
    allow_close: &Rc<Cell<bool>>,
) {
    if !buffer.is_modified() {
        allow_close.set(true);
        window.close();
        return;
    }

    let window = window.clone();
    let parent = parent.clone();
    let allow_close = Rc::clone(allow_close);
    glib::MainContext::default().spawn_local(async move {
        if confirm_discard(&parent).await {
            allow_close.set(true);
            window.close();
        }
    });
}

fn file_label(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}
