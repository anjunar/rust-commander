use rust_i18n::t;

use crate::{
    application::{ActivePanel, AppState, FileOperationKind},
    domain::{sorting::SortColumn, Entry, EntryKind},
};

pub fn ready_status() -> String {
    t!("status.ready").into_owned()
}

pub fn panel_label(panel: ActivePanel) -> String {
    match panel {
        ActivePanel::Left => t!("panel.left").into_owned(),
        ActivePanel::Right => t!("panel.right").into_owned(),
    }
}

pub fn file_operation_label(kind: &FileOperationKind) -> String {
    match kind {
        FileOperationKind::Copy => t!("operation.copy").into_owned(),
        FileOperationKind::Move => t!("operation.move").into_owned(),
        FileOperationKind::Delete => t!("operation.delete").into_owned(),
    }
}

pub fn sort_column_label(column: SortColumn) -> String {
    match column {
        SortColumn::Name => t!("column.name").into_owned(),
        SortColumn::Size => t!("column.size").into_owned(),
        SortColumn::Type => t!("column.type").into_owned(),
        SortColumn::Modified => t!("column.modified").into_owned(),
        SortColumn::Attributes => t!("column.attributes").into_owned(),
    }
}

pub fn entry_size_label(entry: &Entry) -> String {
    entry_size_label_with_calculated(entry, None)
}

pub fn entry_size_label_with_calculated(entry: &Entry, calculated_dir_size: Option<u64>) -> String {
    if entry.is_parent_link {
        "-".into()
    } else if entry.is_dir {
        calculated_dir_size
            .map(crate::fs::reader::format_bytes)
            .unwrap_or_else(|| "-".into())
    } else {
        crate::fs::reader::format_bytes(entry.size_bytes)
    }
}

/// The type column: a file shows its extension (`exe`, `txt`), so sorting by
/// type groups files by extension; a file without one shows "File".
pub fn entry_type_label(entry: &Entry) -> String {
    if entry.is_parent_link {
        return t!("entry.parent").into_owned();
    }

    match entry.kind {
        EntryKind::Directory => t!("entry.folder").into_owned(),
        EntryKind::File => entry
            .extension()
            .unwrap_or_else(|| t!("entry.file").into_owned()),
        EntryKind::Symlink => t!("entry.symlink").into_owned(),
        EntryKind::ArchiveItem => t!("entry.archive_item").into_owned(),
    }
}

#[cfg(test)]
#[path = "../tests/unit/presentation_tests.rs"]
mod tests;

pub fn entry_modified_label(entry: &Entry) -> String {
    entry
        .modified_at
        .map(crate::fs::reader::format_system_time)
        .unwrap_or_else(|| {
            if entry.is_parent_link {
                String::new()
            } else {
                "-".into()
            }
        })
}

pub fn entry_attributes_label(entry: &Entry) -> String {
    if entry.is_parent_link {
        "UP".into()
    } else if entry.attributes.is_empty() {
        "-".into()
    } else {
        entry.attributes.clone()
    }
}

pub fn status_line(state: &AppState) -> String {
    state.status.clone()
}

/// Per-panel footer: marked files/folders counts and total size.
pub fn panel_footer_line(panel: &crate::domain::Panel) -> String {
    let marks: Vec<_> = panel
        .selection_indices()
        .into_iter()
        .filter_map(|index| panel.entries.get(index))
        .filter(|entry| !entry.is_parent_link)
        .collect();
    if marks.is_empty() {
        let total = panel
            .entries
            .len()
            .saturating_sub(panel.entries.iter().filter(|e| e.is_parent_link).count());
        return t!("panel.footer_ready", count = total).into_owned();
    }

    let folders = marks.iter().filter(|entry| entry.is_dir).count();
    let files = marks.len().saturating_sub(folders);
    let bytes: u64 = marks
        .iter()
        .map(|entry| {
            if entry.is_dir {
                panel
                    .calculated_dir_size(&entry.key())
                    .unwrap_or(entry.size_bytes)
            } else {
                entry.size_bytes
            }
        })
        .sum();
    t!(
        "panel.footer_selected",
        folders = folders,
        files = files,
        size = crate::fs::reader::format_bytes(bytes)
    )
    .into_owned()
}
