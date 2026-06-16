use std::{cell::RefCell, path::Path};

use glib::subclass::prelude::*;

use crate::{domain::entry::Entry, platform};

#[derive(Clone, Debug, Default)]
pub struct FileRowData {
    pub name: String,
    pub path: String,
    pub size: String,
    pub type_label: String,
    pub modified: String,
    pub attributes: String,
    pub icon_name: String,
    pub size_bytes: u64,
    pub is_dir: bool,
    pub is_parent_link: bool,
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct FileRowObject {
        pub data: RefCell<FileRowData>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FileRowObject {
        const NAME: &'static str = "RustCommanderFileRowObject";
        type Type = super::FileRowObject;
    }

    impl ObjectImpl for FileRowObject {}
}

glib::wrapper! {
    pub struct FileRowObject(ObjectSubclass<imp::FileRowObject>);
}

impl FileRowObject {
    pub fn new(base_path: &Path, entry: &Entry) -> Self {
        let object: Self = glib::Object::new();
        let data = FileRowData {
            name: entry.name.clone(),
            path: entry.full_path(base_path).display().to_string(),
            size: entry.size_label.clone(),
            type_label: entry.type_label.clone(),
            modified: entry.modified_label.clone(),
            attributes: entry.attributes_label.clone(),
            icon_name: platform::icon_name_for_entry(base_path, entry).to_string(),
            size_bytes: entry.size_bytes,
            is_dir: entry.is_dir,
            is_parent_link: entry.is_parent_link,
        };
        object.imp().data.replace(data);
        object
    }

    pub fn name(&self) -> String {
        self.imp().data.borrow().name.clone()
    }

    pub fn path(&self) -> String {
        self.imp().data.borrow().path.clone()
    }

    pub fn size(&self) -> String {
        self.imp().data.borrow().size.clone()
    }

    pub fn type_label(&self) -> String {
        self.imp().data.borrow().type_label.clone()
    }

    pub fn modified(&self) -> String {
        self.imp().data.borrow().modified.clone()
    }

    pub fn attributes(&self) -> String {
        self.imp().data.borrow().attributes.clone()
    }

    pub fn icon_name(&self) -> String {
        self.imp().data.borrow().icon_name.clone()
    }

    pub fn size_bytes(&self) -> u64 {
        self.imp().data.borrow().size_bytes
    }

    pub fn is_dir(&self) -> bool {
        self.imp().data.borrow().is_dir
    }

    pub fn is_parent_link(&self) -> bool {
        self.imp().data.borrow().is_parent_link
    }
}
