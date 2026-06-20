use std::{path::PathBuf, time::SystemTime};

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

    pub fn full_path(&self, location: &PanelLocation) -> PathBuf {
        location.entry_filesystem_path(self).unwrap_or_default()
    }

    pub fn display_path(&self, location: &PanelLocation) -> String {
        location.entry_display_path(self)
    }
}
