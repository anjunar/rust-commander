use std::cell::RefCell;

use gtk::gdk;
use gtk::glib::{self, subclass::prelude::*};

use crate::{
    domain::{entry::Entry, panel_location::PanelLocation},
    platform,
    presentation,
};

#[derive(Clone, Debug, Default)]
pub struct FileRowData {
    pub name: String,
    pub path: String,
    pub size: String,
    pub type_label: String,
    pub modified: String,
    pub attributes: String,
    pub icon_name: String,
    pub icon_paintable: Option<gdk::Paintable>,
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
    pub fn new(location: &PanelLocation, entry: &Entry) -> Self {
        let object: Self = glib::Object::new();
        object.update(location, entry);
        object
    }

    pub fn update(&self, location: &PanelLocation, entry: &Entry) {
        self.imp().data.replace(Self::build_data(location, entry));
    }

    pub fn matches_entry(&self, location: &PanelLocation, entry: &Entry) -> bool {
        let data = self.imp().data.borrow();
        let path = entry.display_path(location);

        data.name == entry.name
            && data.path == path
            && data.size == presentation::entry_size_label(entry)
            && data.type_label == presentation::entry_type_label(entry)
            && data.modified == presentation::entry_modified_label(entry)
            && data.attributes == presentation::entry_attributes_label(entry)
            && data.size_bytes == entry.size_bytes
            && data.is_dir == entry.is_dir
            && data.is_parent_link == entry.is_parent_link
    }

    fn build_data(location: &PanelLocation, entry: &Entry) -> FileRowData {
        let icon = platform::icon_for_entry(location, entry);
        FileRowData {
            name: entry.name.clone(),
            path: entry.display_path(location),
            size: presentation::entry_size_label(entry),
            type_label: presentation::entry_type_label(entry),
            modified: presentation::entry_modified_label(entry),
            attributes: presentation::entry_attributes_label(entry),
            icon_name: icon.icon_name.to_string(),
            icon_paintable: icon.paintable,
            size_bytes: entry.size_bytes,
            is_dir: entry.is_dir,
            is_parent_link: entry.is_parent_link,
        }
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

    pub fn icon_paintable(&self) -> Option<gdk::Paintable> {
        self.imp().data.borrow().icon_paintable.clone()
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
