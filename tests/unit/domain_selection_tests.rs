use std::{collections::BTreeSet, time::SystemTime};

use super::{
    apply_selection, snapshot_selection, SelectionFallback, SelectionIntent, SelectionModel,
    SelectionSnapshot,
};
use crate::domain::{entry::Entry, entry_key::EntryKey};

#[test]
fn preserve_intent_restores_selection_when_entries_reordered() {
    let selection = SelectionModel::new(BTreeSet::from([1, 2]), Some(2), Some(2));
    let snapshot = snapshot_selection(&selection, &[entry("alpha"), entry("beta"), entry("gamma")]);

    let restored = apply_selection(
        &[entry("gamma"), entry("alpha"), entry("beta")],
        &SelectionIntent::preserve(snapshot),
    );

    assert_eq!(restored.selected_indices().collect::<Vec<_>>(), vec![0, 2]);
    assert_eq!(restored.focus_index(), Some(0));
}

#[test]
fn delete_intent_clamps_to_neighbor() {
    let selection = SelectionModel::single(Some(2));
    let snapshot = snapshot_selection(&selection, &[entry("alpha"), entry("beta"), entry("gamma")]);

    let restored = apply_selection(
        &[entry("alpha"), entry("beta")],
        &SelectionIntent::after_delete(snapshot),
    );

    assert_eq!(restored.selected_indices().collect::<Vec<_>>(), vec![1]);
    assert_eq!(restored.focus_index(), Some(1));
}

#[test]
fn navigation_prefers_parent_link() {
    let restored = apply_selection(
        &[Entry::parent_link(), entry("alpha")],
        &SelectionIntent::for_navigation(None),
    );

    assert_eq!(restored.focus_index(), Some(0));
}

#[test]
fn reveal_prefers_named_entry() {
    let restored = apply_selection(
        &[entry("alpha"), entry("renamed"), entry("gamma")],
        &SelectionIntent {
            snapshot: SelectionSnapshot::default(),
            preferred_cursor: Some(EntryKey::FilesystemName("renamed".into())),
            fallback: SelectionFallback::None,
        },
    );

    assert_eq!(restored.focus_index(), Some(1));
}

#[test]
fn preserve_intent_restores_archive_entries_by_archive_path() {
    let selection = SelectionModel::single(Some(1));
    let snapshot = snapshot_selection(
        &selection,
        &[archive_entry("folder/"), archive_entry("folder/file.txt")],
    );

    let restored = apply_selection(
        &[archive_entry("folder/file.txt"), archive_entry("folder/other.txt")],
        &SelectionIntent::preserve(snapshot),
    );

    assert_eq!(restored.focus_index(), Some(0));
    assert_eq!(restored.selected_indices().collect::<Vec<_>>(), vec![0]);
}

fn entry(name: &str) -> Entry {
    Entry {
        name: name.into(),
        archive_path: None,
        remote_path: None,
        kind: crate::domain::entry::EntryKind::File,
        is_dir: false,
        size_bytes: 1,
        modified_at: Some(SystemTime::now()),
        attributes: String::new(),
        is_parent_link: false,
    }
}

fn archive_entry(path: &str) -> Entry {
    let name = path.trim_end_matches('/').rsplit('/').next().unwrap_or(path);
    Entry {
        name: name.into(),
        archive_path: Some(path.into()),
        remote_path: None,
        kind: if path.ends_with('/') {
            crate::domain::entry::EntryKind::Directory
        } else {
            crate::domain::entry::EntryKind::ArchiveItem
        },
        is_dir: path.ends_with('/'),
        size_bytes: 1,
        modified_at: Some(SystemTime::now()),
        attributes: String::new(),
        is_parent_link: false,
    }
}
