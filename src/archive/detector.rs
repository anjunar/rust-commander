#![allow(dead_code)]

use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArchiveFormat {
    Zip,
    Rar,
    Tar,
    TarGz,
    TarBz2,
    TarXz,
    Gz,
    Bz2,
    Xz,
    Cab,
    Iso,
    Wim,
    Arj,
    Lha,
    Cpio,
    Dmg,
    Chm,
    Msi,
}

pub struct ArchiveFormatDetector;

impl ArchiveFormatDetector {
    pub fn detect(path: &Path) -> Option<ArchiveFormat> {
        if path.is_file() {
            if let Some(format) = Self::detect_from_file_contents(path) {
                return Some(format);
            }

            return Self::detect_from_extension_fallback(path);
        }

        Self::detect_from_extension(path)
    }

    fn detect_from_file_contents(path: &Path) -> Option<ArchiveFormat> {
        let mut file = File::open(path).ok()?;
        let metadata = file.metadata().ok();
        let file_len = metadata.map(|value| value.len()).unwrap_or(0);

        let mut header = [0_u8; 560];
        let header_len = file.read(&mut header).ok()?;
        let header = &header[..header_len];

        if header.starts_with(b"PK\x03\x04")
            || header.starts_with(b"PK\x05\x06")
            || header.starts_with(b"PK\x07\x08")
        {
            return Some(ArchiveFormat::Zip);
        }

        if header.starts_with(b"Rar!\x1A\x07\x00") || header.starts_with(b"Rar!\x1A\x07\x01\x00") {
            return Some(ArchiveFormat::Rar);
        }

        if header.starts_with(&[0x1F, 0x8B]) {
            return Some(Self::compressed_tar_variant(path, ArchiveFormat::Gz));
        }

        if header.starts_with(b"BZh") {
            return Some(Self::compressed_tar_variant(path, ArchiveFormat::Bz2));
        }

        if header.starts_with(&[0xFD, b'7', b'z', b'X', b'Z', 0x00]) {
            return Some(Self::compressed_tar_variant(path, ArchiveFormat::Xz));
        }

        if header.starts_with(b"MSCF") {
            return Some(ArchiveFormat::Cab);
        }

        if header.starts_with(b"MSWIM\0\0\0") {
            return Some(ArchiveFormat::Wim);
        }

        if header.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
            return Self::detect_compound_document_format(path);
        }

        if header.starts_with(b"ITSF") {
            return Some(ArchiveFormat::Chm);
        }

        if header.starts_with(b"070701")
            || header.starts_with(b"070702")
            || header.starts_with(b"070707")
        {
            return Some(ArchiveFormat::Cpio);
        }

        if header_len >= 265 && &header[257..262] == b"ustar" {
            return Some(ArchiveFormat::Tar);
        }

        if file_len >= 0x9001 && Self::matches_at(&mut file, 0x8001, b"CD001").ok()? {
            return Some(ArchiveFormat::Iso);
        }

        Self::detect_from_extension_fallback(path)
    }

    fn detect_compound_document_format(path: &Path) -> Option<ArchiveFormat> {
        let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();
        match extension.as_str() {
            "msi" => Some(ArchiveFormat::Msi),
            _ => None,
        }
    }

    fn compressed_tar_variant(path: &Path, base: ArchiveFormat) -> ArchiveFormat {
        match Self::detect_from_extension(path) {
            Some(ArchiveFormat::TarGz) if base == ArchiveFormat::Gz => ArchiveFormat::TarGz,
            Some(ArchiveFormat::TarBz2) if base == ArchiveFormat::Bz2 => ArchiveFormat::TarBz2,
            Some(ArchiveFormat::TarXz) if base == ArchiveFormat::Xz => ArchiveFormat::TarXz,
            _ => base,
        }
    }

    fn matches_at(file: &mut File, offset: u64, expected: &[u8]) -> std::io::Result<bool> {
        file.seek(SeekFrom::Start(offset))?;
        let mut buffer = vec![0_u8; expected.len()];
        file.read_exact(&mut buffer)?;
        Ok(buffer == expected)
    }

    fn detect_from_extension_fallback(path: &Path) -> Option<ArchiveFormat> {
        match Self::detect_from_extension(path) {
            Some(
                format @ (ArchiveFormat::TarGz
                | ArchiveFormat::TarBz2
                | ArchiveFormat::TarXz
                | ArchiveFormat::Iso
                | ArchiveFormat::Dmg
                | ArchiveFormat::Arj
                | ArchiveFormat::Lha
                | ArchiveFormat::Msi),
            ) => Some(format),
            _ => None,
        }
    }

    fn detect_from_extension(path: &Path) -> Option<ArchiveFormat> {
        let file_name = path.file_name()?.to_string_lossy().to_ascii_lowercase();

        for (suffix, format) in [
            (".tar.gz", ArchiveFormat::TarGz),
            (".tgz", ArchiveFormat::TarGz),
            (".tar.bz2", ArchiveFormat::TarBz2),
            (".tbz2", ArchiveFormat::TarBz2),
            (".tar.xz", ArchiveFormat::TarXz),
            (".txz", ArchiveFormat::TarXz),
            (".zip", ArchiveFormat::Zip),
            (".rar", ArchiveFormat::Rar),
            (".tar", ArchiveFormat::Tar),
            (".gz", ArchiveFormat::Gz),
            (".bz2", ArchiveFormat::Bz2),
            (".xz", ArchiveFormat::Xz),
            (".cab", ArchiveFormat::Cab),
            (".iso", ArchiveFormat::Iso),
            (".wim", ArchiveFormat::Wim),
            (".arj", ArchiveFormat::Arj),
            (".lha", ArchiveFormat::Lha),
            (".lzh", ArchiveFormat::Lha),
            (".cpio", ArchiveFormat::Cpio),
            (".dmg", ArchiveFormat::Dmg),
            (".chm", ArchiveFormat::Chm),
            (".msi", ArchiveFormat::Msi),
        ] {
            if file_name.ends_with(suffix) {
                return Some(format);
            }
        }

        None
    }

    pub fn is_supported_archive(path: &Path) -> bool {
        Self::detect(path).is_some()
    }
}

#[cfg(test)]
mod tests {
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

        assert_eq!(
            ArchiveFormatDetector::detect(&path),
            Some(ArchiveFormat::Zip)
        );

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

        assert_eq!(
            ArchiveFormatDetector::detect(&path),
            Some(ArchiveFormat::Tar)
        );

        fs::remove_file(path).unwrap();
    }

    fn temp_test_file(prefix: &str, suffix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("rust_commander_{prefix}_{unique}{suffix}"))
    }
}
