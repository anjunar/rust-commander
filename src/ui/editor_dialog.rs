#![allow(deprecated)]

use std::{
    fs,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use gtk::{glib, prelude::*};
use sourceview5::{self as sourceview, prelude::*};

use crate::fs::reader::format_bytes;

enum ViewerContent {
    Text { body: String, status: String },
    Hex { body: String, status: String },
}

const VIEW_TEXT_LIMIT_BYTES: usize = 1024 * 1024;
const VIEW_HEX_LIMIT_BYTES: usize = 256 * 1024;
const VIEW_DETECTION_BYTES: usize = 8192;

pub fn edit_file<F>(parent: &gtk::ApplicationWindow, path: PathBuf, on_saved: F) -> Result<()>
where
    F: Fn(PathBuf) + 'static,
{
    let initial_text = read_text_file(&path)?;

    let dialog = gtk::MessageDialog::builder()
        .transient_for(parent)
        .modal(true)
        .text(&format!("Edit {}", file_label(&path)))
        .buttons(gtk::ButtonsType::None)
        .build();
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button("Save", gtk::ResponseType::Accept);
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

pub fn view_file(parent: &gtk::ApplicationWindow, path: PathBuf) -> Result<()> {
    let content = read_viewer_content(&path)?;
    let (title_suffix, body, status_text) = match content {
        ViewerContent::Text { body, status } => ("View", body, status),
        ViewerContent::Hex { body, status } => ("View", body, status),
    };

    let dialog = gtk::MessageDialog::builder()
        .transient_for(parent)
        .modal(true)
        .text(&format!("{title_suffix} {}", file_label(&path)))
        .buttons(gtk::ButtonsType::None)
        .build();
    dialog.add_button("Close", gtk::ResponseType::Close);
    dialog.set_default_size(980, 720);
    dialog.set_default_response(gtk::ResponseType::Close);

    let content_area = dialog.content_area();
    content_area.set_spacing(10);
    content_area.set_margin_top(12);
    content_area.set_margin_bottom(12);
    content_area.set_margin_start(12);
    content_area.set_margin_end(12);

    let path_label = gtk::Label::new(Some(&path.display().to_string()));
    path_label.set_xalign(0.0);
    path_label.add_css_class("path-label");
    content_area.append(&path_label);

    let buffer = sourceview::Buffer::new(None);
    buffer.set_text(&body);
    buffer.set_highlight_syntax(matches!(
        read_viewer_content_type(&path)?,
        ViewerContentType::Text
    ));
    apply_language(&buffer, &path);
    apply_style_scheme(&buffer);

    let view = sourceview::View::with_buffer(&buffer);
    view.set_editable(false);
    view.set_cursor_visible(false);
    view.set_hexpand(true);
    view.set_vexpand(true);
    view.set_monospace(true);
    view.set_show_line_numbers(true);
    view.set_highlight_current_line(true);
    view.set_auto_indent(false);
    view.set_show_right_margin(false);
    view.add_css_class("editor-view");

    let scrolled = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&view)
        .build();
    scrolled.add_css_class("panel-scroller");
    content_area.append(&scrolled);

    let status_label = gtk::Label::new(Some(&status_text));
    status_label.set_xalign(0.0);
    status_label.add_css_class("editor-status");
    content_area.append(&status_label);

    glib::MainContext::default().spawn_local(async move {
        dialog.present();
        view.grab_focus();
        dialog.run_future().await;
        dialog.close();
    });

    Ok(())
}

fn read_text_file(path: &Path) -> Result<String> {
    let bytes =
        fs::read(path).with_context(|| format!("Could not read file {}", path.display()))?;

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

fn read_viewer_content(path: &Path) -> Result<ViewerContent> {
    let content_type = read_viewer_content_type(path)?;
    let file_size = fs::metadata(path)
        .with_context(|| format!("Could not inspect file {}", path.display()))?
        .len();

    match content_type {
        ViewerContentType::Text => {
            let (bytes, truncated) = read_limited_bytes(path, VIEW_TEXT_LIMIT_BYTES)?;
            let body = decode_text_prefix(&bytes)?;
            let status = if truncated {
                format!(
                    "Read-only text view. Showing the first {} of {}.",
                    format_bytes(bytes.len() as u64),
                    format_bytes(file_size)
                )
            } else {
                format!("Read-only text view. {} total.", format_bytes(file_size))
            };
            Ok(ViewerContent::Text { body, status })
        }
        ViewerContentType::Hex => {
            let (bytes, truncated) = read_limited_bytes(path, VIEW_HEX_LIMIT_BYTES)?;
            let status = if truncated {
                format!(
                    "Read-only hex view for binary content. Showing the first {} of {}.",
                    format_bytes(bytes.len() as u64),
                    format_bytes(file_size)
                )
            } else {
                format!(
                    "Read-only hex view for binary content. {} total.",
                    format_bytes(file_size)
                )
            };
            Ok(ViewerContent::Hex {
                body: format_hex_dump(&bytes),
                status,
            })
        }
    }
}

fn read_viewer_content_type(path: &Path) -> Result<ViewerContentType> {
    let (bytes, _) = read_limited_bytes(path, VIEW_DETECTION_BYTES)?;
    Ok(read_viewer_content_type_from_bytes(&bytes))
}

#[derive(Clone, Copy)]
enum ViewerContentType {
    Text,
    Hex,
}

fn read_viewer_content_type_from_bytes(bytes: &[u8]) -> ViewerContentType {
    if bytes.contains(&0) {
        return ViewerContentType::Hex;
    }

    if std::str::from_utf8(bytes).is_ok() {
        ViewerContentType::Text
    } else {
        ViewerContentType::Hex
    }
}

fn read_limited_bytes(path: &Path, limit: usize) -> Result<(Vec<u8>, bool)> {
    let mut file =
        File::open(path).with_context(|| format!("Could not read file {}", path.display()))?;
    let mut bytes = vec![0; limit.saturating_add(1)];
    let read_len = file
        .read(&mut bytes)
        .with_context(|| format!("Could not read file {}", path.display()))?;
    bytes.truncate(read_len);

    let truncated = bytes.len() > limit;
    if truncated {
        bytes.truncate(limit);
    }

    Ok((bytes, truncated))
}

fn decode_text_prefix(bytes: &[u8]) -> Result<String> {
    match std::str::from_utf8(bytes) {
        Ok(text) => Ok(text.to_string()),
        Err(error) => {
            let valid_up_to = error.valid_up_to();
            if valid_up_to == 0 {
                bail!("The selected file is not valid UTF-8 and cannot be viewed as text.")
            }
            std::str::from_utf8(&bytes[..valid_up_to])
                .map(|text| text.to_string())
                .with_context(|| "Could not decode the text preview.")
        }
    }
}

fn format_hex_dump(bytes: &[u8]) -> String {
    const BYTES_PER_LINE: usize = 16;

    if bytes.is_empty() {
        return "00000000  <empty file>".into();
    }

    let mut lines = Vec::new();
    for (offset, chunk) in bytes.chunks(BYTES_PER_LINE).enumerate() {
        let base = offset * BYTES_PER_LINE;
        let hex = chunk
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<Vec<_>>();

        let left = hex.iter().take(8).cloned().collect::<Vec<_>>().join(" ");
        let right = hex.iter().skip(8).cloned().collect::<Vec<_>>().join(" ");

        let left = format!("{left:<23}");
        let right = format!("{right:<23}");

        let ascii = chunk
            .iter()
            .map(|byte| {
                if byte.is_ascii_graphic() || *byte == b' ' {
                    char::from(*byte)
                } else {
                    '.'
                }
            })
            .collect::<String>();

        lines.push(format!("{base:08X}  {left}  {right}  |{ascii}|"));
    }

    lines.join("\n")
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
