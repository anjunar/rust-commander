use std::cell::{Cell, RefCell};

use gtk::gdk;
use gtk::glib::{self, subclass::prelude::*};
use gtk::prelude::*;

use crate::{
    domain::{entry::Entry, panel_location::PanelLocation},
    platform, presentation,
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
    pub icon_resolved: bool,
    pub size_bytes: u64,
    pub is_dir: bool,
    pub is_parent_link: bool,
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct FileRowObject {
        pub data: RefCell<FileRowData>,
        pub marked: Cell<bool>,
        pub name_cell: RefCell<Option<glib::WeakRef<gtk::Box>>>,
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
        Self::new_with_size_label(location, entry, &presentation::entry_size_label(entry))
    }

    pub fn new_with_size_label(location: &PanelLocation, entry: &Entry, size_label: &str) -> Self {
        let object: Self = glib::Object::new();
        object.update_with_size_label(location, entry, size_label);
        object
    }

    pub fn update(&self, location: &PanelLocation, entry: &Entry) {
        self.update_with_size_label(location, entry, &presentation::entry_size_label(entry));
    }

    pub fn update_with_size_label(&self, location: &PanelLocation, entry: &Entry, size_label: &str) {
        self.imp()
            .data
            .replace(Self::build_data(location, entry, size_label));
    }

    pub fn set_size_label(&self, size_label: &str) {
        let mut data = self.imp().data.borrow_mut();
        data.size = size_label.to_string();
    }

    pub fn matches_entry(&self, location: &PanelLocation, entry: &Entry) -> bool {
        let data = self.imp().data.borrow();
        let path = entry.display_path(location);

        data.name == entry.name
            && data.path == path
            && data.type_label == presentation::entry_type_label(entry)
            && data.modified == presentation::entry_modified_label(entry)
            && data.attributes == presentation::entry_attributes_label(entry)
            && data.is_dir == entry.is_dir
            && data.is_parent_link == entry.is_parent_link
    }

    fn build_data(location: &PanelLocation, entry: &Entry, size_label: &str) -> FileRowData {
        FileRowData {
            name: entry.name.clone(),
            path: entry.display_path(location),
            size: size_label.to_string(),
            type_label: presentation::entry_type_label(entry),
            modified: presentation::entry_modified_label(entry),
            attributes: presentation::entry_attributes_label(entry),
            icon_name: platform::fallback_icon_name_for_row(entry.is_parent_link, entry.is_dir)
                .to_string(),
            icon_paintable: None,
            icon_resolved: false,
            size_bytes: entry.size_bytes,
            is_dir: entry.is_dir,
            is_parent_link: entry.is_parent_link,
        }
    }

    pub fn invalidate_icon(&self) {
        let mut data = self.imp().data.borrow_mut();
        data.icon_resolved = false;
        data.icon_paintable = None;
        data.icon_name =
            platform::fallback_icon_name_for_row(data.is_parent_link, data.is_dir).to_string();
    }

    /// Resolve and cache the listing icon on first bind (visible rows only).
    pub fn ensure_icon(&self) {
        let needs_resolve = {
            let data = self.imp().data.borrow();
            !data.icon_resolved
        };
        if !needs_resolve {
            return;
        }

        let (name, is_dir, is_parent_link) = {
            let data = self.imp().data.borrow();
            (data.name.clone(), data.is_dir, data.is_parent_link)
        };

        let entry = Entry {
            name,
            archive_path: None,
            remote_path: None,
            kind: if is_parent_link || is_dir {
                crate::domain::EntryKind::Directory
            } else {
                crate::domain::EntryKind::File
            },
            is_dir,
            size_bytes: 0,
            modified_at: None,
            attributes: String::new(),
            is_parent_link,
        };
        let icon = platform::icon_for_listing_entry(&entry);
        let mut data = self.imp().data.borrow_mut();
        data.icon_name = icon.icon_name.to_string();
        data.icon_paintable = icon.paintable;
        data.icon_resolved = true;
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

    pub fn is_marked(&self) -> bool {
        self.imp().marked.get()
    }

    pub fn set_marked(&self, marked: bool) {
        self.imp().marked.set(marked);
    }

    pub fn set_name_cell(&self, cell: Option<&gtk::Box>) {
        *self.imp().name_cell.borrow_mut() = cell.map(|widget| widget.downgrade());
    }

    pub fn name_cell(&self) -> Option<gtk::Box> {
        self.imp()
            .name_cell
            .borrow()
            .as_ref()
            .and_then(|cell| cell.upgrade())
    }
}
