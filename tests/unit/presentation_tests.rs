use rust_i18n::t;

use super::entry_type_label;
use crate::domain::{Entry, EntryKind};

fn entry(name: &str, kind: EntryKind) -> Entry {
    Entry {
        name: name.into(),
        archive_path: None,
        remote_path: None,
        kind,
        is_dir: kind == EntryKind::Directory,
        size_bytes: 0,
        modified_at: None,
        attributes: String::new(),
        is_parent_link: false,
    }
}

#[test]
fn type_column_shows_the_extension_of_a_file() {
    assert_eq!(
        entry_type_label(&entry("Setup.EXE", EntryKind::File)),
        "exe"
    );
    assert_eq!(
        entry_type_label(&entry("notes.txt", EntryKind::File)),
        "txt"
    );
}

#[test]
fn type_column_falls_back_to_the_kind() {
    assert_eq!(
        entry_type_label(&entry("README", EntryKind::File)),
        t!("entry.file")
    );
    assert_eq!(
        entry_type_label(&entry("docs.d", EntryKind::Directory)),
        t!("entry.folder")
    );
    assert_eq!(entry_type_label(&Entry::parent_link()), t!("entry.parent"));
}
