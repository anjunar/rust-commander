use super::safe_join_extract_path;

#[test]
fn accepts_normal_relative_paths() {
    let path = safe_join_extract_path(std::path::Path::new("/tmp/out"), "dir/file.txt").unwrap();
    assert_eq!(
        path,
        std::path::Path::new("/tmp/out").join("dir").join("file.txt")
    );
}

#[test]
fn blocks_parent_traversal() {
    assert!(safe_join_extract_path(std::path::Path::new("/tmp/out"), "../evil.exe").is_err());
}

#[test]
fn blocks_absolute_unix_paths() {
    assert!(safe_join_extract_path(std::path::Path::new("/tmp/out"), "/etc/passwd").is_err());
}

#[test]
fn blocks_windows_drive_paths() {
    assert!(
        safe_join_extract_path(
            std::path::Path::new("C:\\target"),
            "C:\\Windows\\system32\\cmd.exe"
        )
        .is_err()
    );
}

#[test]
fn blocks_unc_paths() {
    assert!(
        safe_join_extract_path(
            std::path::Path::new("C:\\target"),
            "\\\\server\\share\\evil.dll"
        )
        .is_err()
    );
}

#[test]
fn blocks_nested_parent_traversal() {
    assert!(
        safe_join_extract_path(std::path::Path::new("/tmp/out"), "folder/../../evil.exe").is_err()
    );
}

#[test]
fn accepts_plain_file_name() {
    let path = safe_join_extract_path(std::path::Path::new("/tmp/out"), "normal.txt").unwrap();
    assert_eq!(path, std::path::Path::new("/tmp/out").join("normal.txt"));
}
