use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use super::{ViewerMode, ViewerState};
use crate::config::ViewerConfig;

fn temp_file_path(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("rcommander_{name}_{unique}.tmp"))
}

#[test]
fn scrolling_does_not_underflow() {
    let path = temp_file_path("viewer_scroll");
    fs::write(&path, b"one\ntwo\n").unwrap();

    let mut state = ViewerState::open(&path, &ViewerConfig::default()).unwrap();
    state.scroll_line_up();
    assert_eq!(state.first_visible_line(), 0);

    state.page_up();
    assert_eq!(state.first_visible_line(), 0);

    state.scroll_line_down();
    assert_eq!(state.first_visible_line(), 0);

    state.toggle_hex_mode();
    assert_eq!(state.mode(), ViewerMode::Hex);
    state.scroll_line_up();
    assert_eq!(state.first_visible_line(), 0);

    fs::remove_file(path).unwrap();
}

#[test]
fn estimates_more_than_indexed_lines_for_large_text_files() {
    let path = temp_file_path("viewer_estimate");
    let content = "line\n".repeat(2_000);
    fs::write(&path, content).unwrap();

    let state = ViewerState::open(&path, &ViewerConfig::default()).unwrap();

    assert!(state.estimated_total_lines() > 100);

    fs::remove_file(path).unwrap();
}
