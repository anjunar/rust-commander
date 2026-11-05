use std::path::{Path, PathBuf};

use super::ArchiveFormat;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArchiveFamily {
    Zip,
    Rar,
    Iso,
    TarLike,
    SevenZip,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArchiveLayout {
    SingleFile,
    MultiPart { first_part: PathBuf },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArchiveSupport {
    Supported,
    NotSupportedYet { reason: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveProbe {
    pub family: ArchiveFamily,
    pub detected_format: Option<ArchiveFormat>,
    pub layout: ArchiveLayout,
    pub support: ArchiveSupport,
}

impl ArchiveProbe {
    pub fn supported(family: ArchiveFamily, detected_format: Option<ArchiveFormat>) -> Self {
        Self {
            family,
            detected_format,
            layout: ArchiveLayout::SingleFile,
            support: ArchiveSupport::Supported,
        }
    }

    pub fn multipart_unsupported(
        family: ArchiveFamily,
        first_part: PathBuf,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            family,
            detected_format: None,
            layout: ArchiveLayout::MultiPart { first_part },
            support: ArchiveSupport::NotSupportedYet {
                reason: reason.into(),
            },
        }
    }
}

pub fn archive_family_for_format(format: ArchiveFormat) -> ArchiveFamily {
    match format {
        ArchiveFormat::Zip => ArchiveFamily::Zip,
        ArchiveFormat::Rar => ArchiveFamily::Rar,
        ArchiveFormat::Iso => ArchiveFamily::Iso,
        ArchiveFormat::Tar
        | ArchiveFormat::TarGz
        | ArchiveFormat::TarBz2
        | ArchiveFormat::TarXz
        | ArchiveFormat::Gz
        | ArchiveFormat::Bz2
        | ArchiveFormat::Xz
        | ArchiveFormat::Cab
        | ArchiveFormat::Wim
        | ArchiveFormat::Cpio => ArchiveFamily::TarLike,
        ArchiveFormat::SevenZip => ArchiveFamily::SevenZip,
        ArchiveFormat::Arj
        | ArchiveFormat::Lha
        | ArchiveFormat::Dmg
        | ArchiveFormat::Chm
        | ArchiveFormat::Msi => ArchiveFamily::Other,
    }
}

pub fn probe_multipart_zip(path: &Path) -> Option<ArchiveProbe> {
    let file_name = path.file_name()?.to_string_lossy().to_ascii_lowercase();

    if file_name.contains(".zip.")
        && file_name
            .rsplit('.')
            .next()
            .is_some_and(|part| part.chars().all(|c| c.is_ascii_digit()))
    {
        return Some(ArchiveProbe::multipart_unsupported(
            ArchiveFamily::Zip,
            path.to_path_buf(),
            format!(
                "Opening multi-part ZIP/ZIP64 archives like {}",
                path.display()
            ),
        ));
    }

    // Byte suffix: string indexing would panic on non-ASCII names ending in `.7z`.
    let bytes = file_name.as_bytes();
    if bytes.len() >= 4 {
        let suffix = &bytes[bytes.len() - 4..];
        if suffix[0] == b'.'
            && suffix[1] == b'z'
            && suffix[2].is_ascii_digit()
            && suffix[3].is_ascii_digit()
        {
            return Some(ArchiveProbe::multipart_unsupported(
                ArchiveFamily::Zip,
                path.to_path_buf(),
                format!(
                    "Opening multi-part ZIP/ZIP64 archives like {}",
                    path.display()
                ),
            ));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn cyrillic_sevenzip_name_is_not_multipart_zip() {
        let path = Path::new(r"C:\111\логическая цепочка.7z");
        assert!(probe_multipart_zip(path).is_none());
        // Must not panic (old code sliced mid-UTF-8 on this name).
        let registry = crate::archive::ArchiveBackendRegistry::with_default_backends();
        let _ = registry.probe_path(path);
        assert!(registry.backend_that_can_open(path).is_some());
    }

    #[test]
    fn ascii_split_zip_part_is_detected() {
        let path = Path::new(r"C:\data\archive.z01");
        let probe = probe_multipart_zip(path).expect("multipart zip");
        assert!(matches!(
            probe.support,
            ArchiveSupport::NotSupportedYet { .. }
        ));
    }
}
