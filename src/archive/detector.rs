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
    SevenZip,
}

pub struct ArchiveFormatDetector;

impl ArchiveFormatDetector {
    pub fn detect(path: &Path) -> Option<ArchiveFormat> {
        // Prefer extension so known archive types (especially `.7z` on Windows)
        // are never opened just for format detection.
        if let Some(format) = Self::detect_from_extension(path) {
            return Some(format);
        }

        if path.is_file() {
            return Self::detect_from_file_contents(path);
        }

        None
    }

    /// Extension-only detection — no filesystem reads.
    pub fn detect_from_extension(path: &Path) -> Option<ArchiveFormat> {
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
            (".7z", ArchiveFormat::SevenZip),
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

    pub fn is_seven_zip_path(path: &Path) -> bool {
        matches!(
            Self::detect_from_extension(path),
            Some(ArchiveFormat::SevenZip)
        )
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

        if header.starts_with(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]) {
            return Some(ArchiveFormat::SevenZip);
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

        None
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
}

#[cfg(test)]
#[path = "../../tests/unit/archive_detector_tests.rs"]
mod tests;
