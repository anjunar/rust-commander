use std::path::Path;

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
    use super::{ArchiveFormat, ArchiveFormatDetector};

    #[test]
    fn detects_common_extensions() {
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
}
