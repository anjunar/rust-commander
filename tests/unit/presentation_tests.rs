use rust_i18n::t;

use super::entry_type_label;
use crate::{
    application::{OperationError, PathAccess},
    domain::{Entry, EntryKind},
};

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

#[test]
fn a_refusal_is_told_by_kind_of_access_and_names_the_path() {
    let path = r"C:\Program Files\note.txt";
    let text = |access| {
        super::operation_error_text(&OperationError::access_denied(access, path, "raw detail"))
    };

    assert_eq!(
        text(PathAccess::Read),
        t!("error.access_denied_read", path = path)
    );
    assert_eq!(
        text(PathAccess::Write),
        t!("error.access_denied_write", path = path)
    );
    assert_eq!(
        text(PathAccess::Delete),
        t!("error.access_denied_delete", path = path)
    );
    assert!(text(PathAccess::Write).contains(path));
}

#[test]
fn any_other_failure_is_shown_as_reported() {
    let error = OperationError::execution("Could not write note.txt: disk full");

    assert_eq!(super::operation_error_text(&error), error.detail());
}
