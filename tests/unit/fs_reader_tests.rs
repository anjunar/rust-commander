use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::domain::entry::EntryKind;

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
fn directory_total_size_sums_nested_files() {
    let path = temp_dir_path("dir_total_size");
    fs::create_dir_all(path.join("sub")).unwrap();
    fs::write(path.join("a.txt"), b"12345").unwrap();
    fs::write(path.join("sub").join("b.txt"), b"abc").unwrap();

    let total = super::directory_total_size(&path);
    let _ = fs::remove_dir_all(&path);

    assert_eq!(total, 8);
}

#[test]
fn read_entry_returns_none_for_missing_path() {
    let path = temp_dir_path("missing_entry");
    let entry = read_entry(&path.join("missing.txt"), true).unwrap();
    assert!(entry.is_none());
}

#[test]
fn symlink_to_directory_is_classified_as_directory_link() {
    let path = temp_dir_path("symlink_dir");
    fs::create_dir_all(path.join("target")).unwrap();
    fs::write(path.join("target").join("inside.txt"), b"ok").unwrap();
    let link = path.join("link");

    #[cfg(target_os = "windows")]
    {
        std::os::windows::fs::symlink_dir(path.join("target"), &link).unwrap();
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::os::unix::fs::symlink(path.join("target"), &link).unwrap();
    }

    let entries = read_entries(&path, true).unwrap();
    let link_entry = entries.iter().find(|entry| entry.name == "link").unwrap();
    let _ = fs::remove_dir_all(&path);

    assert!(link_entry.is_dir);
    assert_eq!(link_entry.kind, EntryKind::Symlink);
}

#[test]
fn symlink_to_file_is_classified_as_file_link() {
    let path = temp_dir_path("symlink_file");
    fs::create_dir(&path).unwrap();
    fs::write(path.join("target.txt"), b"payload").unwrap();
    let link = path.join("link.txt");

    #[cfg(target_os = "windows")]
    {
        std::os::windows::fs::symlink_file(path.join("target.txt"), &link).unwrap();
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::os::unix::fs::symlink(path.join("target.txt"), &link).unwrap();
    }

    let entries = read_entries(&path, true).unwrap();
    let link_entry = entries
        .iter()
        .find(|entry| entry.name == "link.txt")
        .unwrap();
    let _ = fs::remove_dir_all(&path);

    assert!(!link_entry.is_dir);
    assert_eq!(link_entry.kind, EntryKind::Symlink);
}

#[cfg(target_os = "windows")]
#[test]
fn windows_folder_shortcut_is_classified_as_directory_link() {
    use std::process::Command;

    let path = temp_dir_path("folder_shortcut");
    fs::create_dir_all(path.join("target")).unwrap();
    fs::write(path.join("target").join("inside.txt"), b"ok").unwrap();
    let shortcut = path.join("folder.lnk");
    let target = path.join("target");

    let script = format!(
        "$s = (New-Object -ComObject WScript.Shell).CreateShortcut('{}'); $s.TargetPath = '{}'; $s.Save()",
        shortcut.display().to_string().replace('\'', "''"),
        target.display().to_string().replace('\'', "''"),
    );
    let status = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .status()
        .expect("powershell");
    assert!(status.success());

    let entries = read_entries(&path, true).unwrap();
    let shortcut_entry = entries
        .iter()
        .find(|entry| entry.name.eq_ignore_ascii_case("folder.lnk"))
        .expect("shortcut entry");
    let resolved = super::resolve_directory_navigation_path(&shortcut);
    let _ = fs::remove_dir_all(&path);

    assert!(shortcut_entry.is_dir);
    assert_eq!(shortcut_entry.kind, EntryKind::Symlink);
    assert_eq!(resolved, target);
}

#[cfg(target_os = "windows")]
#[test]
fn restricted_windows_junction_resolves_to_readable_target() {
    let path = std::path::Path::new(r"C:\Documents and Settings");
    if !path.exists() {
        return;
    }

    let resolved = super::resolve_directory_navigation_path(path);
    assert_eq!(resolved, std::path::PathBuf::from(r"C:\Users"));
    assert!(std::fs::read_dir(&resolved).is_ok());
}
