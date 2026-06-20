use std::{path::PathBuf, time::SystemTime};

use crate::domain::panel_location::PanelLocation;
use crate::domain::EntryKey;
use crate::remote::RemotePath;

#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub archive_path: Option<String>,
    pub remote_path: Option<RemotePath>,
    pub is_dir: bool,
    pub size_bytes: u64,
    pub size_label: String,
    pub type_label: String,
    pub modified_at: Option<SystemTime>,
    pub modified_label: String,
    pub attributes_label: String,
    pub is_parent_link: bool,
}

impl Entry {
    pub fn key(&self) -> EntryKey {
        EntryKey::for_entry(self)
    }

    pub fn parent_link(type_label: impl Into<String>) -> Self {
        Self {
            name: "..".into(),
            archive_path: None,
            remote_path: None,
            is_dir: true,
            size_bytes: 0,
            size_label: "-".into(),
            type_label: type_label.into(),
            modified_at: None,
            modified_label: String::new(),
            attributes_label: "UP".into(),
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
