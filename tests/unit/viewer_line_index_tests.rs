use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use super::LineIndex;
use crate::viewer::file_source::FileSource;

fn temp_file_path(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("rcommander_{name}_{unique}.tmp"))
}

#[test]
fn indexes_multiple_lines() {
    let path = temp_file_path("line_index_multi");
    fs::write(&path, b"one\ntwo\nthree").unwrap();

    let source = FileSource::open(&path).unwrap();
    let mut index = LineIndex::new();
    index.ensure_complete(&source).unwrap();

    assert_eq!(index.line_count(), 3);
    assert_eq!(index.line_start(0), Some(0));
    assert_eq!(index.line_start(1), Some(4));
    assert_eq!(index.line_start(2), Some(8));

    fs::remove_file(path).unwrap();
}

#[test]
fn indexes_empty_file() {
    let path = temp_file_path("line_index_empty");
    fs::write(&path, b"").unwrap();

    let source = FileSource::open(&path).unwrap();
    let mut index = LineIndex::new();
    index.ensure_complete(&source).unwrap();

    assert_eq!(index.line_count(), 1);
    assert_eq!(index.line_start(0), Some(0));
    assert!(index.is_complete());

    fs::remove_file(path).unwrap();
}

#[test]
fn tracks_indexed_offset() {
    let path = temp_file_path("line_index_progress");
    fs::write(&path, b"one\ntwo\nthree\nfour\n").unwrap();

    let source = FileSource::open(&path).unwrap();
    let index = LineIndex::build_initial(&source, 5, 5).unwrap();

    assert!(index.indexed_until() >= 5);

    fs::remove_file(path).unwrap();
}
