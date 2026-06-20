use super::{sort_entries, SortColumn, SortDirection, SortState};
use crate::domain::{Entry, EntryKind};

fn file(name: &str, is_dir: bool) -> Entry {
    Entry {
        name: name.into(),
        archive_path: None,
        remote_path: None,
        kind: if is_dir {
            EntryKind::Directory
        } else {
            EntryKind::File
        },
        is_dir,
        size_bytes: 0,
        modified_at: None,
        attributes: String::new(),
        is_parent_link: false,
    }
}

#[test]
fn folders_first_keeps_directories_above_files() {
    let mut entries = vec![file("zeta.txt", false), file("alpha", true)];

    sort_entries(
        &mut entries,
        SortState {
            column: SortColumn::Name,
            direction: SortDirection::Ascending,
        },
        true,
    );

    assert!(entries[0].is_dir);
    assert!(!entries[1].is_dir);
}

#[test]
fn disabling_folders_first_uses_plain_sort_order() {
    let mut entries = vec![file("zeta", true), file("alpha.txt", false)];

    sort_entries(
        &mut entries,
        SortState {
            column: SortColumn::Name,
            direction: SortDirection::Ascending,
        },
        false,
    );

    assert_eq!(entries[0].name, "alpha.txt");
    assert_eq!(entries[1].name, "zeta");
}
