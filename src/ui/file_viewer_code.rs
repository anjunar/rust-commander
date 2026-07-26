use std::path::Path;

use anyhow::Result;
use gtk::prelude::*;
use sourceview5::{self as sourceview, prelude::*};

use crate::{
    config::ThemePreference,
    ui::{
        file_viewer_common::{
            append_status, build_viewer_chrome, file_size, install_escape_close,
            present_with_focus, read_text_file_limited,
        },
        sourceview_lang::{apply_language, apply_language_id, apply_style_scheme, has_guessable_language},
    },
};

pub(crate) fn can_open_as_code(path: &Path, size_bytes: u64, max_bytes: u64) -> bool {
    size_bytes <= max_bytes && has_guessable_language(path)
}

pub(crate) fn open(
    parent: &gtk::ApplicationWindow,
    path: &Path,
    max_bytes: u64,
    theme: ThemePreference,
    mode: &str,
) -> Result<()> {
    let text = read_text_file_limited(path, max_bytes)?;
    open_highlighted_text(parent, path, &text, None, mode, theme)
}

pub(crate) fn open_highlighted_text(
    parent: &gtk::ApplicationWindow,
    path: &Path,
    text: &str,
    language_id: Option<&str>,
    mode: &str,
    theme: ThemePreference,
) -> Result<()> {
    let chrome = build_viewer_chrome(parent, path);

    let buffer = sourceview::Buffer::new(None);
    buffer.set_text(text);
    buffer.set_highlight_syntax(true);
    if let Some(language_id) = language_id {
        apply_language_id(&buffer, language_id);
    } else {
        apply_language(&buffer, path);
    }
    apply_style_scheme(&buffer, theme);

    let view = sourceview::View::with_buffer(&buffer);
    view.set_editable(false);
    view.set_cursor_visible(false);
    view.set_monospace(true);
    view.set_show_line_numbers(true);
    view.set_highlight_current_line(true);
    view.set_wrap_mode(gtk::WrapMode::None);
    view.set_hexpand(true);
    view.set_vexpand(true);
    view.add_css_class("editor-view");
    view.set_left_margin(8);
    view.set_top_margin(8);
    view.set_bottom_margin(8);

    let scrolled = gtk::ScrolledWindow::builder()
        .child(&view)
        .hexpand(true)
        .vexpand(true)
        .build();
    scrolled.add_css_class("panel-scroller");
    chrome.content.append(&scrolled);

    append_status(&chrome.content, mode, file_size(path));
    install_escape_close(&chrome.window);
    present_with_focus(chrome.window, view);
    Ok(())
}
