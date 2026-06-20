use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use super::render_text_lines;
use crate::viewer::{file_source::FileSource, line_index::LineIndex};

fn temp_file_path(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("rcommander_{name}_{unique}.tmp"))
}

#[test]
fn renders_invalid_utf8_lossy() {
    let path = temp_file_path("text_lossy");
    fs::write(&path, [0x66, 0x6F, 0x80, 0x6F, b'\n']).unwrap();

    let source = FileSource::open(&path).unwrap();
    let mut index = LineIndex::new();
    let render = render_text_lines(&source, &mut index, 0, 1, 0).unwrap();

    assert_eq!(render.lines, vec!["fo�o".to_string()]);

    fs::remove_file(path).unwrap();
}
