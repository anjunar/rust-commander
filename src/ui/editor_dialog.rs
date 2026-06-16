#![allow(deprecated)]

use std::{fs, path::{Path, PathBuf}};

use anyhow::{Context, Result, bail};
use gtk::{glib, prelude::*};
use sourceview5::{self as sourceview, prelude::*};

pub fn edit_file<F>(parent: &gtk::ApplicationWindow, path: PathBuf, on_saved: F) -> Result<()>
where
    F: Fn(PathBuf) + 'static,
{
    let initial_text = read_text_file(&path)?;

    let dialog = gtk::Dialog::with_buttons(
        Some(&format!("Edit {}", file_label(&path))),
        Some(parent),
        gtk::DialogFlags::MODAL,
        &[
            ("Cancel", gtk::ResponseType::Cancel),
            ("Save", gtk::ResponseType::Accept),
        ],
    );
    dialog.set_default_size(980, 720);
    dialog.set_default_response(gtk::ResponseType::Accept);

    let content = dialog.content_area();
    content.set_spacing(10);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);

    let path_label = gtk::Label::new(Some(&path.display().to_string()));
    path_label.set_xalign(0.0);
    path_label.add_css_class("path-label");
    content.append(&path_label);

    let buffer = sourceview::Buffer::new(None);
    buffer.set_text(&initial_text);
    buffer.set_highlight_syntax(true);
    buffer.set_modified(false);
    apply_language(&buffer, &path);
    apply_style_scheme(&buffer);

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

    let scrolled = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&view)
        .build();
    scrolled.add_css_class("panel-scroller");
    content.append(&scrolled);

    let status_label = gtk::Label::new(Some("UTF-8 text file"));
    status_label.set_xalign(0.0);
    status_label.add_css_class("editor-status");
    content.append(&status_label);

    let parent = parent.clone();
    glib::MainContext::default().spawn_local(async move {
        dialog.present();
        view.grab_focus();

        loop {
            let response = dialog.run_future().await;
            match response {
                gtk::ResponseType::Accept => {
                    let text = current_buffer_text(&buffer);
                    match save_text_file(&path, &text) {
                        Ok(()) => {
                            buffer.set_modified(false);
                            on_saved(path.clone());
                            dialog.close();
                            break;
                        }
                        Err(error) => {
                            crate::ui::dialogs::show_error(
                                &parent,
                                "Could not save file",
                                &error.to_string(),
                            );
                        }
                    }
                }
                _ => {
                    if !buffer.is_modified() || confirm_discard(&parent).await {
                        dialog.close();
                        break;
                    }
                }
            }
        }
    });

    Ok(())
}

fn read_text_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path)
        .with_context(|| format!("Could not read file {}", path.display()))?;

    if bytes.contains(&0) {
        bail!("The selected file appears to be binary and cannot be edited as text.");
    }

    String::from_utf8(bytes).with_context(|| {
        format!(
            "The selected file is not valid UTF-8 and cannot be edited in the text editor: {}",
            path.display()
        )
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
    let dialog = gtk::MessageDialog::builder()
        .transient_for(parent)
        .modal(true)
        .message_type(gtk::MessageType::Question)
        .buttons(gtk::ButtonsType::None)
        .text("Discard changes?")
        .secondary_text("There are unsaved changes in the editor.")
        .build();
    dialog.add_button("Keep editing", gtk::ResponseType::Cancel);
    dialog.add_button("Discard", gtk::ResponseType::Accept);
    dialog.set_default_response(gtk::ResponseType::Cancel);

    let response = dialog.run_future().await;
    dialog.close();
    response == gtk::ResponseType::Accept
}

fn file_label(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}
