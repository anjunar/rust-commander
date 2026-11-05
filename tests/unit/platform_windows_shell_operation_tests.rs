use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use super::{perform, ShellAction, ShellOperation, ShellOutcome, ShellTarget};

// `elevate` stays off here: with it on, Windows would prompt during the tests.
fn operation(action: ShellAction, sources: Vec<PathBuf>) -> ShellOperation {
    ShellOperation {
        action,
        sources,
        elevate: false,
        owner: 0,
    }
}

fn target(directory: &Path, name: Option<&str>) -> ShellTarget {
    ShellTarget {
        directory: directory.to_path_buf(),
        name: name.map(Into::into),
        overwrite_silently: false,
    }
}

#[test]
fn copies_files_and_folders_into_a_directory() {
    let root = test_dir("copy");
    let (file, folder, destination) = fixture(&root);

    let copy = ShellAction::Copy(target(&destination, None));
    let outcome = perform(&operation(copy, vec![file.clone(), folder.clone()])).unwrap();

    assert_eq!(outcome, ShellOutcome::Completed);
    assert_eq!(
        fs::read_to_string(destination.join("note.txt")).unwrap(),
        "note"
    );
    assert_eq!(
        fs::read_to_string(destination.join("folder").join("inner.txt")).unwrap(),
        "inner"
    );
    assert!(file.exists() && folder.exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn copy_with_a_new_name_uses_that_name() {
    let root = test_dir("copy_as");
    let (file, _, destination) = fixture(&root);

    let copy = ShellAction::Copy(target(&destination, Some("renamed.txt")));
    perform(&operation(copy, vec![file])).unwrap();

    assert!(destination.join("renamed.txt").exists());
    assert!(!destination.join("note.txt").exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn move_leaves_nothing_at_the_source() {
    let root = test_dir("move");
    let (file, _, destination) = fixture(&root);

    let move_item = ShellAction::Move(target(&destination, None));
    perform(&operation(move_item, vec![file.clone()])).unwrap();

    assert!(!file.exists());
    assert!(destination.join("note.txt").exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn permanent_delete_removes_files_and_folders() {
    let root = test_dir("delete");
    let (file, folder, _) = fixture(&root);

    let delete = ShellAction::Delete { recycle: false };
    let outcome = perform(&operation(delete, vec![file.clone(), folder.clone()])).unwrap();

    assert_eq!(outcome, ShellOutcome::Completed);
    assert!(!file.exists() && !folder.exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn a_missing_source_is_an_error_that_names_it() {
    let root = test_dir("missing");
    let missing = root.join("not-there.txt");

    let delete = ShellAction::Delete { recycle: false };
    let error = perform(&operation(delete, vec![missing])).unwrap_err();

    assert!(format!("{error:#}").contains("not-there.txt"));
    let _ = fs::remove_dir_all(root);
}

fn fixture(root: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let file = root.join("note.txt");
    let folder = root.join("folder");
    let destination = root.join("destination");
    fs::write(&file, "note").unwrap();
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("inner.txt"), "inner").unwrap();
    fs::create_dir_all(&destination).unwrap();
    (file, folder, destination)
}

fn test_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("rcommander_shell_op_{name}_{nanos}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}
