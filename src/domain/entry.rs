use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::domain::panel_location::PanelLocation;
use crate::domain::EntryKey;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
    ArchiveItem,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub archive_path: Option<String>,
    pub remote_path: Option<String>,
    pub kind: EntryKind,
    pub is_dir: bool,
    pub size_bytes: u64,
    pub modified_at: Option<SystemTime>,
    pub attributes: String,
    pub is_parent_link: bool,
}

impl Entry {
    pub fn key(&self) -> EntryKey {
        EntryKey::for_entry(self)
    }

    pub fn parent_link() -> Self {
        Self {
            name: "..".into(),
            archive_path: None,
            remote_path: None,
            kind: EntryKind::Directory,
            is_dir: true,
            size_bytes: 0,
            modified_at: None,
            attributes: String::new(),
            is_parent_link: true,
        }
    }

    /// The extension of a file in lower case, such as `exe` for `Setup.EXE`.
    ///
    /// Directories and special entries have none, and neither have names
    /// without a dot after the first character: `README`, `.gitignore`.
    pub fn extension(&self) -> Option<String> {
        if self.is_dir || self.is_parent_link || self.kind != EntryKind::File {
            return None;
        }
        Path::new(&self.name)
            .extension()
            .and_then(|extension| extension.to_str())
            .filter(|extension| !extension.is_empty())
            .map(str::to_lowercase)
    }

    /// The order of the type column: directories, then symlinks and other
    /// special entries, then files grouped by extension, files without an
    /// extension first.
    pub fn type_sort_key(&self) -> (u8, String) {
        if self.is_dir {
            return (0, String::new());
        }
        match self.kind {
            EntryKind::File => (2, self.extension().unwrap_or_default()),
            _ => (1, String::new()),
        }
    }

    pub fn full_path(&self, location: &PanelLocation) -> PathBuf {
        location.entry_filesystem_path(self).unwrap_or_default()
    }

    pub fn display_path(&self, location: &PanelLocation) -> String {
        location.entry_display_path(self)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/domain_entry_tests.rs"]
mod tests;
