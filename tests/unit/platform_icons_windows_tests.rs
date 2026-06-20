use std::path::PathBuf;

use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL};

use super::*;

#[test]
fn icon_request_uses_real_paths_for_existing_entries() {
    let path = PathBuf::from(r"C:\temp\app.exe");
    let key = IconKey::ExistingPath(path.clone());
    let (requested_path, attributes, use_file_attributes) = icon_request(&key);

    assert_eq!(requested_path, path.as_path());
    assert_eq!(attributes, 0);
    assert!(!use_file_attributes);
}

#[test]
fn icon_request_maps_virtual_directory_and_file_requests() {
    let (directory_path, directory_attributes, directory_hint) = icon_request(&IconKey::Directory);
    assert_eq!(directory_path, std::path::Path::new("folder"));
    assert_eq!(directory_attributes, FILE_ATTRIBUTE_DIRECTORY);
    assert!(directory_hint);

    let (file_path, file_attributes, file_hint) = icon_request(&IconKey::File);
    assert_eq!(file_path, std::path::Path::new("file"));
    assert_eq!(file_attributes, FILE_ATTRIBUTE_NORMAL);
    assert!(file_hint);
}
