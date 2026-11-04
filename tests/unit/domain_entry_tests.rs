use super::{Entry, EntryKind};

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
fn extension_is_the_lower_case_suffix_of_a_file() {
    assert_eq!(
        entry("Setup.EXE", EntryKind::File).extension().as_deref(),
        Some("exe")
    );
    assert_eq!(
        entry("archive.tar.gz", EntryKind::File)
            .extension()
            .as_deref(),
        Some("gz")
    );
}

#[test]
fn names_without_a_suffix_have_no_extension() {
    assert_eq!(entry("README", EntryKind::File).extension(), None);
    assert_eq!(entry(".gitignore", EntryKind::File).extension(), None);
    assert_eq!(entry("trailing.", EntryKind::File).extension(), None);
}

#[test]
fn only_files_have_an_extension() {
    assert_eq!(entry("photos.d", EntryKind::Directory).extension(), None);
    assert_eq!(entry("link.exe", EntryKind::Symlink).extension(), None);
    assert_eq!(Entry::parent_link().extension(), None);
}

#[test]
fn type_sort_key_puts_directories_first_and_groups_files_by_extension() {
    let directory = entry("zeta", EntryKind::Directory).type_sort_key();
    let symlink = entry("link", EntryKind::Symlink).type_sort_key();
    let plain = entry("README", EntryKind::File).type_sort_key();
    let exe = entry("app.exe", EntryKind::File).type_sort_key();
    let txt = entry("notes.TXT", EntryKind::File).type_sort_key();

    assert!(directory < symlink);
    assert!(symlink < plain);
    assert!(plain < exe);
    assert!(exe < txt);
    assert_eq!(txt, (2, "txt".to_string()));
}
