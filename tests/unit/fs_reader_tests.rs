use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use super::{read_entries, read_entry};

fn temp_dir_path(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("rcommander_{name}_{unique}"))
}

#[test]
fn hides_dotfiles_when_disabled() {
    let path = temp_dir_path("hidden_listing");
    fs::create_dir(&path).unwrap();
    fs::write(path.join("visible.txt"), b"visible").unwrap();
    fs::write(path.join(".hidden.txt"), b"hidden").unwrap();

    let entries = read_entries(&path, false).unwrap();
    let _ = fs::remove_dir_all(&path);

    assert!(entries.iter().any(|entry| entry.name == "visible.txt"));
    assert!(!entries.iter().any(|entry| entry.name == ".hidden.txt"));
}

#[test]
fn includes_dotfiles_when_enabled() {
    let path = temp_dir_path("show_hidden_listing");
    fs::create_dir(&path).unwrap();
    fs::write(path.join(".hidden.txt"), b"hidden").unwrap();

    let entries = read_entries(&path, true).unwrap();
    let _ = fs::remove_dir_all(&path);

    assert!(entries.iter().any(|entry| entry.name == ".hidden.txt"));
}

#[test]
fn read_entry_returns_none_for_missing_path() {
    let path = temp_dir_path("missing_entry");
    let entry = read_entry(&path.join("missing.txt"), true).unwrap();
    assert!(entry.is_none());
}
