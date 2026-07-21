use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::domain::entry::Entry;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortColumn {
    #[default]
    Name,
    Size,
    Type,
    Modified,
    Attributes,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    #[default]
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
        SortColumn::Type => compare_text(file_extension(&a.name), file_extension(&b.name))
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

pub fn file_extension(name: &str) -> &str {
    let Some((stem, extension)) = name.rsplit_once('.') else {
        return "";
    };
    if stem.is_empty() {
        ""
    } else {
        extension
    }
}

#[cfg(test)]
#[path = "../../tests/unit/domain_sorting_tests.rs"]
mod tests;
