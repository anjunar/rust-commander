use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use sevenz_rust2::compress_to_path;

use super::SevenZipBackend;
use crate::archive::ArchiveBackend;

fn unique_temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("rcommander-sevenz-{label}-{nanos}"));
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

#[test]
fn sevenz_backend_detects_sevenz_paths() {
    let backend = SevenZipBackend::new();
    assert!(backend.can_open(Path::new("archive.7z")));
    assert!(!backend.can_open(Path::new("archive.zip")));
    assert!(!backend.can_open(Path::new("archive.rar")));
}

#[test]
fn sevenz_backend_lists_and_extracts_entries() {
    let root = unique_temp_dir("roundtrip");
    let source_dir = root.join("source");
    let archive_path = root.join("sample.7z");
    let extract_dir = root.join("extract");
    fs::create_dir_all(source_dir.join("nested")).expect("source nested");
    fs::write(source_dir.join("readme.txt"), b"hello sevenzip").expect("write file");
    fs::write(source_dir.join("nested").join("data.bin"), b"\x01\x02\x03").expect("write nested");

    compress_to_path(&source_dir, &archive_path).expect("compress 7z");

    let backend = SevenZipBackend::new();
    assert!(backend.can_open(&archive_path));

    let session = backend.open(&archive_path).expect("open 7z");
    let names: Vec<_> = session
        .cached_entries()
        .iter()
        .map(|entry| entry.archive_path.as_str())
        .collect();
    assert!(
        names.iter().any(|name| name.ends_with("readme.txt")),
        "missing readme.txt in {names:?}"
    );
    assert!(
        names.iter().any(|name| name.contains("data.bin")),
        "missing data.bin in {names:?}"
    );

    fs::create_dir_all(&extract_dir).expect("extract dir");
    let readme_entry = session
        .cached_entries()
        .iter()
        .find(|entry| entry.archive_path.ends_with("readme.txt"))
        .expect("readme entry")
        .archive_path
        .clone();
    backend
        .extract_entry(&session, &readme_entry, &extract_dir)
        .expect("extract readme");

    let extracted = extract_dir.join(&readme_entry);
    let content = fs::read_to_string(&extracted).expect("read extracted");
    assert_eq!(content, "hello sevenzip");

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn sevenz_backend_rejects_non_sevenz_open() {
    let root = unique_temp_dir("reject");
    let fake = root.join("not-an-archive.zip");
    let mut file = fs::File::create(&fake).expect("create fake");
    file.write_all(b"PK\x03\x04not really").expect("write fake");

    let backend = SevenZipBackend::new();
    assert!(backend.open(&fake).is_err());

    let _ = fs::remove_dir_all(&root);
}
