use std::{path::PathBuf, time::SystemTime};

use rust_i18n::t;

use crate::domain::panel_location::PanelLocation;

#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub archive_path: Option<String>,
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
    pub fn parent_link() -> Self {
        Self {
            name: "..".into(),
            archive_path: None,
            is_dir: true,
            size_bytes: 0,
            size_label: "-".into(),
            type_label: t!("entry.parent").into_owned(),
            modified_at: None,
            modified_label: String::new(),
            attributes_label: "UP".into(),
            is_parent_link: true,
        }
    }

    pub fn full_path(&self, location: &PanelLocation) -> PathBuf {
        location.entry_display_path(self)
    }
}
