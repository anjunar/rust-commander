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

pub fn file_operation_verb(kind: &FileOperationKind) -> String {
    match kind {
        FileOperationKind::Copy => t!("operation.copy_verb").into_owned(),
        FileOperationKind::Move => t!("operation.move_verb").into_owned(),
        FileOperationKind::Delete => t!("operation.delete_verb").into_owned(),
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
    if entry.is_parent_link || entry.is_dir {
        "-".into()
    } else {
        crate::fs::reader::format_bytes(entry.size_bytes)
    }
}

pub fn entry_type_label(entry: &Entry) -> String {
    if entry.is_parent_link {
        return t!("entry.parent").into_owned();
    }

    match entry.kind {
        EntryKind::Directory => t!("entry.folder").into_owned(),
        EntryKind::File => t!("entry.file").into_owned(),
        EntryKind::Symlink => t!("entry.symlink").into_owned(),
        EntryKind::ArchiveItem => t!("entry.archive_item").into_owned(),
    }
}

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
    let active = state.active_panel();
    let inactive = state.inactive_panel();
    let active_selected = active.selected_count();
    let inactive_selected = inactive.selected_count();

    match (active_selected, inactive_selected) {
        (0, 0) => state.status.clone(),
        (active_count, 0) => t!(
            "status.selected_active",
            status = state.status.as_str(),
            count = active_count
        )
        .into_owned(),
        (0, inactive_count) => t!(
            "status.selected_inactive",
            status = state.status.as_str(),
            count = inactive_count
        )
        .into_owned(),
        (active_count, inactive_count) => t!(
            "status.selected_both",
            status = state.status.as_str(),
            active = active_count,
            inactive = inactive_count
        )
        .into_owned(),
    }
}
