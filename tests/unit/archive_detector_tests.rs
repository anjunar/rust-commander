use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use super::{ArchiveFormat, ArchiveFormatDetector};

#[test]
fn detects_common_extensions_for_nonexistent_paths() {
    assert_eq!(
        ArchiveFormatDetector::detect(std::path::Path::new("backup.tar.gz")),
        Some(ArchiveFormat::TarGz)
    );
    assert_eq!(
        ArchiveFormatDetector::detect(std::path::Path::new("docs.ZIP")),
        Some(ArchiveFormat::Zip)
    );
    assert_eq!(
        ArchiveFormatDetector::detect(std::path::Path::new("movie.rar")),
        Some(ArchiveFormat::Rar)
    );
    assert_eq!(
        ArchiveFormatDetector::detect(std::path::Path::new("image.iso")),
        Some(ArchiveFormat::Iso)
    );
    assert_eq!(
        ArchiveFormatDetector::detect(std::path::Path::new("music.lzh")),
        Some(ArchiveFormat::Lha)
    );
    assert!(ArchiveFormatDetector::detect(std::path::Path::new("notes.txt")).is_none());
}

#[test]
fn detects_zip_by_signature_without_extension() {
    let path = temp_test_file("archive_signature", "");
    fs::write(&path, b"PK\x03\x04test-payload").unwrap();

    assert_eq!(ArchiveFormatDetector::detect(&path), Some(ArchiveFormat::Zip));

    fs::remove_file(path).unwrap();
}

#[test]
fn rejects_fake_zip_with_matching_extension() {
    let path = temp_test_file("fake_archive", ".zip");
    fs::write(&path, b"this is definitely not a zip archive").unwrap();

    assert_eq!(ArchiveFormatDetector::detect(&path), None);

    fs::remove_file(path).unwrap();
}

#[test]
fn detects_tar_by_ustar_signature() {
    let path = temp_test_file("tar_signature", ".bin");
    let mut content = vec![0_u8; 512];
    content[257..262].copy_from_slice(b"ustar");
    fs::write(&path, content).unwrap();

    assert_eq!(ArchiveFormatDetector::detect(&path), Some(ArchiveFormat::Tar));

    fs::remove_file(path).unwrap();
}

fn temp_test_file(prefix: &str, suffix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("rust_commander_{prefix}_{unique}{suffix}"))
}
