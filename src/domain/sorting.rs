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

pub fn sort_entries(entries: &mut [Entry], sort_state: SortState) {
    entries.sort_by(|a, b| compare_entries(a, b, sort_state));
}

fn compare_entries(a: &Entry, b: &Entry, sort_state: SortState) -> Ordering {
    if a.is_parent_link || b.is_parent_link {
        return match (a.is_parent_link, b.is_parent_link) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => Ordering::Equal,
        };
    }

    match (a.is_dir, b.is_dir) {
        (true, false) => return Ordering::Less,
        (false, true) => return Ordering::Greater,
        _ => {}
    }

    let ordering = match sort_state.column {
        SortColumn::Name => compare_text(&a.name, &b.name),
        SortColumn::Size => a
            .size_bytes
            .cmp(&b.size_bytes)
            .then_with(|| compare_text(&a.name, &b.name)),
        SortColumn::Type => {
            compare_text(&a.type_label, &b.type_label).then_with(|| compare_text(&a.name, &b.name))
        }
        SortColumn::Modified => a
            .modified_at
            .cmp(&b.modified_at)
            .then_with(|| compare_text(&a.name, &b.name)),
        SortColumn::Attributes => a
            .attributes_label
            .cmp(&b.attributes_label)
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
