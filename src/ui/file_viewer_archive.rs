use std::path::Path;

use anyhow::{Context, Result};
use gtk::prelude::*;
use rust_i18n::t;

use crate::{
    archive::{ArchiveEntryKind, ArchiveService},
    fs::reader::format_bytes,
    ui::file_viewer_common::{
        append_status, build_viewer_chrome, file_size, install_escape_close, present_with_focus,
    },
};

pub(crate) fn open(
    parent: &gtk::ApplicationWindow,
    path: &Path,
    archive_service: &ArchiveService,
) -> Result<()> {
    let session = archive_service
        .open_archive(path)
        .with_context(|| format!("Could not open archive {}", path.display()))?;

    let mut entries = session.cached_entries().to_vec();
    entries.sort_by(|left, right| {
        let left_dir = matches!(left.kind, ArchiveEntryKind::Directory);
        let right_dir = matches!(right.kind, ArchiveEntryKind::Directory);
        right_dir.cmp(&left_dir).then_with(|| {
            left.archive_path
                .to_ascii_lowercase()
                .cmp(&right.archive_path.to_ascii_lowercase())
        })
    });

    let listing = format_archive_listing(&entries);
    let chrome = build_viewer_chrome(parent, path);

    let buffer = gtk::TextBuffer::new(None);
    buffer.set_text(&listing);
    let view = gtk::TextView::with_buffer(&buffer);
    view.set_editable(false);
    view.set_cursor_visible(false);
    view.set_monospace(true);
    view.set_wrap_mode(gtk::WrapMode::None);
    view.set_hexpand(true);
    view.set_vexpand(true);
    view.add_css_class("editor-view");
    view.set_left_margin(10);
    view.set_top_margin(8);
    view.set_bottom_margin(8);

    let scrolled = gtk::ScrolledWindow::builder()
        .child(&view)
        .hexpand(true)
        .vexpand(true)
        .build();
    scrolled.add_css_class("panel-scroller");
    chrome.content.append(&scrolled);

    append_status(
        &chrome.content,
        &t!("viewer.mode_archive", count = entries.len()).into_owned(),
        file_size(path),
    );
    install_escape_close(&chrome.window);
    present_with_focus(chrome.window, view);
    Ok(())
}

fn format_archive_listing(entries: &[crate::archive::ArchiveEntry]) -> String {
    if entries.is_empty() {
        return t!("viewer.archive_empty").into_owned();
    }

    let name_width = entries
        .iter()
        .map(|entry| entry.archive_path.chars().count())
        .max()
        .unwrap_or(8)
        .clamp(8, 80);

    let mut out = String::new();
    out.push_str(&format!(
        "{:<width$}  {:>10}  {}\n",
        t!("viewer.archive_col_name").into_owned(),
        t!("viewer.archive_col_size").into_owned(),
        t!("viewer.archive_col_type").into_owned(),
        width = name_width
    ));
    out.push_str(&format!("{}\n", "-".repeat(name_width + 2 + 10 + 2 + 12)));

    for entry in entries {
        let kind = match entry.kind {
            ArchiveEntryKind::Directory => t!("viewer.archive_type_dir").into_owned(),
            ArchiveEntryKind::File => t!("viewer.archive_type_file").into_owned(),
            #[cfg(not(target_os = "windows"))]
            ArchiveEntryKind::Symlink => t!("viewer.archive_type_link").into_owned(),
            #[cfg(not(target_os = "windows"))]
            ArchiveEntryKind::Unknown => t!("viewer.archive_type_unknown").into_owned(),
        };
        let size = match entry.kind {
            ArchiveEntryKind::Directory => "-".to_string(),
            _ => format_bytes(entry.size),
        };
        let name = truncate_middle(&entry.archive_path, name_width);
        out.push_str(&format!("{name:<name_width$}  {size:>10}  {kind}\n"));
    }

    out
}

fn truncate_middle(value: &str, max_chars: usize) -> String {
    let count = value.chars().count();
    if count <= max_chars {
        return value.to_string();
    }
    if max_chars <= 3 {
        return value.chars().take(max_chars).collect();
    }
    let keep = max_chars - 1;
    let head = keep / 2;
    let tail = keep - head;
    let mut chars = value.chars();
    let prefix: String = chars.by_ref().take(head).collect();
    let suffix: String = value
        .chars()
        .rev()
        .take(tail)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("{prefix}…{suffix}")
}
