use std::{fs, time::Duration};

use super::*;
use crate::config::IconStyle;

#[test]
fn set_icon_style_clears_cache_and_updates_style() {
    set_icon_style(IconStyle::Symbolic);
    assert_eq!(icon_style(), IconStyle::Symbolic);
    set_icon_style(IconStyle::System);
    assert_eq!(icon_style(), IconStyle::System);
    set_icon_style(IconStyle::Symbolic);
    assert_eq!(icon_style(), IconStyle::Symbolic);
}

#[test]
fn path_stamp_tracks_size_changes() {
    let mut path = std::env::temp_dir();
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    path.push(format!(
        "rcommander-icon-stamp-{}-{timestamp}.tmp",
        std::process::id()
    ));

    fs::write(&path, b"a").unwrap();
    let first = path_stamp(&path).unwrap();

    std::thread::sleep(Duration::from_millis(5));
    fs::write(&path, b"updated").unwrap();
    let second = path_stamp(&path).unwrap();

    let _ = fs::remove_file(&path);

    assert_ne!(first, second);
}
