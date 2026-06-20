use std::path::Path;

use super::UnrarBackend;
use crate::archive::ArchiveBackend;

#[test]
fn rar_backend_detects_rar_paths() {
    let backend = UnrarBackend::new();
    assert!(backend.can_open(Path::new("archive.rar")));
    assert!(!backend.can_open(Path::new("archive.zip")));
}
