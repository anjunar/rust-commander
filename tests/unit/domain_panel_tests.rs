use std::{collections::BTreeSet, time::SystemTime};

use super::Panel;
use crate::domain::{
    entry::{Entry, EntryKind},
    selection::SelectionModel,
    sorting::{SortColumn, SortDirection},
    PanelLocation,
};

#[test]
fn replace_entries_restores_focus_and_selection_by_key() {
    let mut panel = Panel::new(
        PanelLocation::filesystem(std::path::PathBuf::from("/tmp")),
        vec![entry("alpha"), entry("beta"), entry("gamma"), entry("delta")],
        true,
    );

    panel.selection = SelectionModel::new(BTreeSet::from([1, 2]), Some(2), Some(1));

    panel.replace_entries(vec![entry("delta"), entry("gamma"), entry("beta"), entry("alpha")]);

    let selected = panel
        .selection_indices()
        .into_iter()
        .map(|index| panel.entries[index].name.clone())
        .collect::<Vec<_>>();
    assert_eq!(selected, vec!["beta".to_string(), "delta".to_string()]);
    assert_eq!(panel.selected_entry().unwrap().name, "delta");
    assert_eq!(panel.selection.focus_index(), Some(2));
    assert_eq!(panel.selection.anchor_index(), panel.selection.focus_index());
}

#[test]
fn sort_keeps_selected_entry_by_name() {
    let mut panel = Panel::new(
        PanelLocation::filesystem(std::path::PathBuf::from("/tmp")),
        vec![
            sized_entry("b.txt", 30),
            sized_entry("a.txt", 10),
            sized_entry("c.txt", 20),
        ],
        true,
    );

    panel.select_single(1);
    panel.set_sort_state(SortColumn::Size, SortDirection::Ascending);

    assert_eq!(panel.selected_entry().unwrap().name, "b.txt");
}

#[test]
fn replace_entries_does_not_select_first_row_when_selection_disappears() {
    let mut panel = Panel::new(
        PanelLocation::filesystem(std::path::PathBuf::from("/tmp")),
        vec![entry("alpha"), entry("beta"), entry("gamma")],
        true,
    );

    panel.select_single(1);
    panel.replace_entries(vec![entry("alpha"), entry("gamma")]);

    assert!(panel.selection_indices().is_empty());
    assert!(panel.selected_entry().is_none());
    assert_eq!(panel.selection.focus_index(), None);
}

#[test]
fn navigate_to_prefers_parent_when_no_history_exists() {
    let mut panel = Panel::new(
        PanelLocation::filesystem(std::path::PathBuf::from("/tmp")),
        vec![entry("alpha")],
        true,
    );

    panel.navigate_to(
        PanelLocation::filesystem(std::path::PathBuf::from("/tmp/child")),
        vec![Entry::parent_link(), entry("beta")],
    );

    assert_eq!(panel.selected_entry().unwrap().name, "..");
}

#[test]
fn rename_selected_entry_reveals_new_name_after_sort() {
    let mut panel = Panel::new(
        PanelLocation::filesystem(std::path::PathBuf::from("/tmp")),
        vec![entry("b.txt"), entry("c.txt")],
        true,
    );

    panel.select_single(1);
    panel.rename_selected_entry("a.txt").unwrap();

    assert_eq!(panel.selected_entry().unwrap().name, "a.txt");
    assert_eq!(panel.selection.focus_index(), Some(0));
}

#[test]
fn navigate_back_restores_remembered_child_directory() {
    let mut panel = Panel::new(
        PanelLocation::filesystem(std::path::PathBuf::from("/tmp")),
        vec![entry("alpha"), dir_entry("child")],
        true,
    );

    panel.select_single(0);
    panel.navigate_to(
        PanelLocation::filesystem(std::path::PathBuf::from("/tmp/child")),
        vec![Entry::parent_link(), entry("nested")],
    );
    panel.navigate_to(
        PanelLocation::filesystem(std::path::PathBuf::from("/tmp")),
        vec![entry("alpha"), dir_entry("child"), entry("zeta")],
    );

    assert_eq!(panel.selected_entry().unwrap().name, "child");
}

#[test]
fn queued_delete_selection_clamps_to_parent_link_when_last_item_disappears() {
    let mut panel = Panel::new(
        PanelLocation::filesystem(std::path::PathBuf::from("/tmp/child")),
        vec![Entry::parent_link(), entry("last.txt")],
        true,
    );

    panel.select_single(1);
    panel.queue_delete_selection();
    panel.replace_entries(vec![Entry::parent_link()]);

    assert_eq!(panel.selected_entry().unwrap().name, "..");
    assert_eq!(panel.selection.focus_index(), Some(0));
}

#[test]
fn queued_delete_selection_allows_empty_result() {
    let mut panel = Panel::new(
        PanelLocation::filesystem(std::path::PathBuf::from("/tmp")),
        vec![entry("only.txt")],
        true,
    );

    panel.select_single(0);
    panel.queue_delete_selection();
    panel.replace_entries(vec![]);

    assert!(panel.selected_entry().is_none());
    assert!(panel.selection_indices().is_empty());
}

#[test]
fn apply_filesystem_entry_change_updates_existing_entry() {
    let mut panel = Panel::new(
        PanelLocation::filesystem(std::path::PathBuf::from("/tmp")),
        vec![entry("alpha.txt")],
        true,
    );

    let changed = panel.apply_filesystem_entry_change(
        std::path::Path::new("/tmp/alpha.txt"),
        Some(Entry {
            name: "alpha.txt".into(),
            archive_path: None,
            remote_path: None,
            kind: EntryKind::File,
            is_dir: false,
            size_bytes: 99,
            modified_at: Some(SystemTime::now()),
            attributes: String::new(),
            is_parent_link: false,
        }),
    );

    assert!(changed);
    assert_eq!(panel.entries.len(), 1);
    assert_eq!(panel.entries[0].size_bytes, 99);
}

#[test]
fn apply_filesystem_entry_change_removes_missing_entry() {
    let mut panel = Panel::new(
        PanelLocation::filesystem(std::path::PathBuf::from("/tmp")),
        vec![entry("alpha.txt"), entry("beta.txt")],
        true,
    );

    let changed = panel.apply_filesystem_entry_change(std::path::Path::new("/tmp/alpha.txt"), None);

    assert!(changed);
    assert_eq!(
        panel
            .entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        vec!["beta.txt"]
    );
}

fn entry(name: &str) -> Entry {
    sized_entry(name, 1)
}

fn dir_entry(name: &str) -> Entry {
    Entry {
        name: name.into(),
        archive_path: None,
        remote_path: None,
        kind: EntryKind::Directory,
        is_dir: true,
        size_bytes: 0,
        modified_at: Some(SystemTime::now()),
        attributes: String::new(),
        is_parent_link: false,
    }
}

fn sized_entry(name: &str, size_bytes: u64) -> Entry {
    Entry {
        name: name.into(),
        archive_path: None,
        remote_path: None,
        kind: EntryKind::File,
        is_dir: false,
        size_bytes,
        modified_at: Some(SystemTime::now()),
        attributes: String::new(),
        is_parent_link: false,
    }
}
