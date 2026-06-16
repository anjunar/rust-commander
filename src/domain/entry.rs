use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
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
            is_dir: true,
            size_bytes: 0,
            size_label: "-".into(),
            type_label: "Parent".into(),
            modified_at: None,
            modified_label: String::new(),
            attributes_label: "UP".into(),
            is_parent_link: true,
        }
    }

    pub fn full_path(&self, base_path: &Path) -> PathBuf {
        if self.is_parent_link {
            base_path.parent().unwrap_or(base_path).to_path_buf()
        } else {
            base_path.join(&self.name)
        }
    }
}
