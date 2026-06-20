use std::cmp::Ordering;

use crate::domain::entry::Entry;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SortColumn {
    Name,
    Size,
    Type,
    Modified,
    Attributes,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SortState {
    pub column: SortColumn,
    pub direction: SortDirection,
}

impl Default for SortState {
    fn default() -> Self {
        Self {
            column: SortColumn::Name,
            direction: SortDirection::Ascending,
        }
    }
}

impl SortState {
    pub fn toggled_for(self, column: SortColumn) -> Self {
        if self.column == column {
            Self {
                column,
                direction: match self.direction {
                    SortDirection::Ascending => SortDirection::Descending,
                    SortDirection::Descending => SortDirection::Ascending,
                },
            }
        } else {
            Self {
                column,
                direction: SortDirection::Ascending,
            }
        }
    }
}

pub fn sort_entries(entries: &mut [Entry], sort_state: SortState, folders_first: bool) {
    entries.sort_by(|a, b| compare_entries(a, b, sort_state, folders_first));
}

fn compare_entries(a: &Entry, b: &Entry, sort_state: SortState, folders_first: bool) -> Ordering {
    if a.is_parent_link || b.is_parent_link {
        return match (a.is_parent_link, b.is_parent_link) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => Ordering::Equal,
        };
    }

    if folders_first {
        match (a.is_dir, b.is_dir) {
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            _ => {}
        }
    }

    let ordering = match sort_state.column {
        SortColumn::Name => compare_text(&a.name, &b.name),
        SortColumn::Size => a
            .size_bytes
            .cmp(&b.size_bytes)
            .then_with(|| compare_text(&a.name, &b.name)),
        SortColumn::Type => a
            .kind
            .cmp(&b.kind)
            .then_with(|| compare_text(&a.name, &b.name)),
        SortColumn::Modified => a
            .modified_at
            .cmp(&b.modified_at)
            .then_with(|| compare_text(&a.name, &b.name)),
        SortColumn::Attributes => a
            .attributes
            .cmp(&b.attributes)
            .then_with(|| compare_text(&a.name, &b.name)),
    };

    match sort_state.direction {
        SortDirection::Ascending => ordering,
        SortDirection::Descending => ordering.reverse(),
    }
}

fn compare_text(a: &str, b: &str) -> Ordering {
    a.to_lowercase().cmp(&b.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::{sort_entries, SortColumn, SortDirection, SortState};
    use crate::domain::{Entry, EntryKind};

    fn file(name: &str, is_dir: bool) -> Entry {
        Entry {
            name: name.into(),
            archive_path: None,
            remote_path: None,
            kind: if is_dir {
                EntryKind::Directory
            } else {
                EntryKind::File
            },
            is_dir,
            size_bytes: 0,
            modified_at: None,
            attributes: String::new(),
            is_parent_link: false,
        }
    }

    #[test]
    fn folders_first_keeps_directories_above_files() {
        let mut entries = vec![file("zeta.txt", false), file("alpha", true)];

        sort_entries(
            &mut entries,
            SortState {
                column: SortColumn::Name,
                direction: SortDirection::Ascending,
            },
            true,
        );

        assert!(entries[0].is_dir);
        assert!(!entries[1].is_dir);
    }

    #[test]
    fn disabling_folders_first_uses_plain_sort_order() {
        let mut entries = vec![file("zeta", true), file("alpha.txt", false)];

        sort_entries(
            &mut entries,
            SortState {
                column: SortColumn::Name,
                direction: SortDirection::Ascending,
            },
            false,
        );

        assert_eq!(entries[0].name, "alpha.txt");
        assert_eq!(entries[1].name, "zeta");
    }
}
