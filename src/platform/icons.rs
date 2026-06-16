use std::path::Path;

use crate::domain::entry::Entry;

pub fn icon_name_for_entry(base_path: &Path, entry: &Entry) -> &'static str {
    if entry.is_parent_link {
        return "go-up-symbolic";
    }

    let path = base_path.join(&entry.name);
    if entry.is_dir || path.is_dir() {
        "folder-symbolic"
    } else {
        "text-x-generic-symbolic"
    }
}
