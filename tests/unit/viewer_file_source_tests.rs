use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use super::FileSource;

fn temp_file_path(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("rcommander_{name}_{unique}.tmp"))
}

#[test]
fn read_at_reads_from_start() {
    let path = temp_file_path("read_start");
    fs::write(&path, b"hello world").unwrap();

    let source = FileSource::open(&path).unwrap();
    let bytes = source.read_at(0, 5).unwrap();

    assert_eq!(bytes, b"hello");

    fs::remove_file(path).unwrap();
}

#[test]
fn read_at_clamps_near_eof() {
    let path = temp_file_path("read_eof");
    fs::write(&path, b"abcdef").unwrap();

    let source = FileSource::open(&path).unwrap();
    let bytes = source.read_at(4, 16).unwrap();

    assert_eq!(bytes, b"ef");

    fs::remove_file(path).unwrap();
}
