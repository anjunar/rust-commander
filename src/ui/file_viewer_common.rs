use std::{fs, path::Path};

use anyhow::{bail, Context, Result};
use gtk::{gdk, glib, prelude::*};
use rust_i18n::t;

use crate::{fs::reader::format_bytes, ui::dialogs::build_independent_window};

pub(crate) struct ViewerChrome {
    pub window: gtk::ApplicationWindow,
    pub title_label: gtk::Label,
    pub path_label: gtk::Label,
    pub content: gtk::Box,
}

pub(crate) fn build_viewer_chrome(parent: &gtk::ApplicationWindow, path: &Path) -> ViewerChrome {
    let independent = build_independent_window(parent, &t!("viewer.window_title"), 980, 720);
    let window = independent.window;
    let title_label = independent.title_label;
    let content = independent.content;
    independent.actions.set_visible(false);

    let path_label = gtk::Label::new(None);
    path_label.set_xalign(0.0);
    path_label.add_css_class("path-label");
    content.append(&path_label);

    let chrome = ViewerChrome {
        window,
        title_label,
        path_label,
        content,
    };
    set_viewer_path(&chrome, path);
    chrome
}

pub(crate) fn set_viewer_path(chrome: &ViewerChrome, path: &Path) {
    let title = file_title(path);
    chrome.window.set_title(Some(&title));
    chrome.title_label.set_label(&title);
    chrome.title_label.set_tooltip_text(Some(&title));
    let path_text = path.display().to_string();
    chrome.path_label.set_label(&path_text);
    chrome.path_label.set_tooltip_text(Some(&path_text));
}

pub(crate) fn file_title(path: &Path) -> String {
    t!(
        "viewer.title",
        file = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("?")
    )
    .into_owned()
}

pub(crate) fn append_status(content: &gtk::Box, mode: &str, size_bytes: u64) -> gtk::Label {
    let status_label = gtk::Label::new(Some(&t!(
        "viewer.status_markdown",
        mode = mode,
        size = format_bytes(size_bytes)
    )));
    status_label.set_xalign(0.0);
    status_label.add_css_class("editor-status");
    status_label.set_hexpand(true);
    content.append(&status_label);
    status_label
}

pub(crate) fn install_escape_close(window: &gtk::ApplicationWindow) {
    let key_controller = gtk::EventControllerKey::new();
    {
        let window = window.clone();
        key_controller.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape {
                window.close();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
    }
    window.add_controller(key_controller);
}

pub(crate) fn present_with_focus(window: gtk::ApplicationWindow, focus: impl IsA<gtk::Widget>) {
    let focus = focus.upcast::<gtk::Widget>();
    glib::idle_add_local_once(move || {
        window.present();
        let _ = focus.grab_focus();
    });
}

pub(crate) fn read_text_file_limited(path: &Path, max_bytes: u64) -> Result<String> {
    let meta =
        fs::metadata(path).with_context(|| format!("Could not read file {}", path.display()))?;
    if meta.len() > max_bytes {
        bail!(
            "{}",
            t!(
                "viewer.file_too_large",
                size = format_bytes(meta.len()),
                limit = format_bytes(max_bytes)
            )
        );
    }

    let bytes =
        fs::read(path).with_context(|| format!("Could not read file {}", path.display()))?;
    if bytes.contains(&0) {
        bail!("{}", t!("viewer.invalid_utf8_for_view"));
    }
    String::from_utf8(bytes).with_context(|| t!("viewer.invalid_utf8_for_view").into_owned())
}

pub(crate) fn file_size(path: &Path) -> u64 {
    fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}
