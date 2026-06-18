use std::ffi::OsString;

use crate::domain::entry::Entry;

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum EntryKey {
    ParentLink,
    FilesystemName(OsString),
    ArchiveEntry(String),
}

impl EntryKey {
    pub fn for_entry(entry: &Entry) -> Self {
        if entry.is_parent_link {
            Self::ParentLink
        } else if let Some(path) = &entry.archive_path {
            Self::ArchiveEntry(path.clone())
        } else {
            Self::FilesystemName(entry.name.clone().into())
        }
    }
}

