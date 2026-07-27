use std::collections::BTreeSet;

use crate::domain::{entry::Entry, entry_key::EntryKey};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SelectionSnapshot {
    pub cursor_key: Option<EntryKey>,
    pub selected_keys: BTreeSet<EntryKey>,
    pub cursor_index: Option<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectionIntent {
    pub snapshot: SelectionSnapshot,
    pub preferred_cursor: Option<EntryKey>,
    pub fallback: SelectionFallback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionFallback {
    None,
    NeighborOrClamp,
    ParentOrFirst,
    First,
}

#[derive(Clone, Debug, Default)]
pub struct SelectionModel {
    anchor_index: Option<usize>,
    focused_index: Option<usize>,
    /// Marked rows (independent of the cursor), as in classic file commanders.
    selected_indices: BTreeSet<usize>,
}

impl SelectionIntent {
    pub fn preserve(snapshot: SelectionSnapshot) -> Self {
        Self {
            preferred_cursor: snapshot.cursor_key.clone(),
            snapshot,
            fallback: SelectionFallback::None,
        }
    }

    pub fn reveal(preferred_cursor: EntryKey, snapshot: SelectionSnapshot) -> Self {
        Self {
            preferred_cursor: Some(preferred_cursor),
            snapshot,
            fallback: SelectionFallback::NeighborOrClamp,
        }
    }

    pub fn after_delete(snapshot: SelectionSnapshot) -> Self {
        Self {
            snapshot: SelectionSnapshot {
                selected_keys: BTreeSet::new(),
                ..snapshot
            },
            preferred_cursor: None,
            fallback: SelectionFallback::NeighborOrClamp,
        }
    }

    /// Keep the cursor, drop marks (e.g. after a successful copy).
    pub fn clear_marks(snapshot: SelectionSnapshot) -> Self {
        Self {
            preferred_cursor: snapshot.cursor_key.clone(),
            snapshot: SelectionSnapshot {
                selected_keys: BTreeSet::new(),
                ..snapshot
            },
            fallback: SelectionFallback::NeighborOrClamp,
        }
    }

    pub fn for_navigation(remembered_cursor: Option<EntryKey>) -> Self {
        Self {
            snapshot: SelectionSnapshot::default(),
            preferred_cursor: remembered_cursor,
            fallback: SelectionFallback::ParentOrFirst,
        }
    }

    pub fn select_first() -> Self {
        Self {
            snapshot: SelectionSnapshot::default(),
            preferred_cursor: None,
            fallback: SelectionFallback::First,
        }
    }
}

impl SelectionModel {
    pub fn new(
        selected_indices: BTreeSet<usize>,
        focused_index: Option<usize>,
        anchor_index: Option<usize>,
    ) -> Self {
        Self {
            anchor_index,
            focused_index,
            selected_indices,
        }
    }

    /// Cursor on `index` with no marks.
    pub fn single(index: Option<usize>) -> Self {
        match index {
            Some(index) => Self::from_cursor(BTreeSet::new(), Some(index)),
            None => Self::default(),
        }
    }

    pub fn from_cursor(selected_indices: BTreeSet<usize>, focused_index: Option<usize>) -> Self {
        Self {
            anchor_index: focused_index,
            focused_index,
            selected_indices,
        }
    }

    #[cfg(test)]
    pub fn anchor_index(&self) -> Option<usize> {
        self.anchor_index
    }

    pub fn focus_index(&self) -> Option<usize> {
        self.focused_index
    }

    pub fn primary_index(&self) -> Option<usize> {
        self.focused_index
            .or_else(|| self.selected_indices.iter().next().copied())
    }

    pub fn selected_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.selected_indices.iter().copied()
    }

    pub fn is_marked(&self, index: usize) -> bool {
        self.selected_indices.contains(&index)
    }

    /// Replace marks only; cursor is unchanged.
    pub fn set_marks(&mut self, indices: impl IntoIterator<Item = usize>, len: usize) {
        self.selected_indices = indices.into_iter().filter(|index| *index < len).collect();
    }

    /// Move the cursor without clearing marks.
    pub fn set_focus(&mut self, index: usize, len: usize) {
        if index < len {
            self.anchor_index = Some(index);
            self.focused_index = Some(index);
        }
    }

    pub fn toggle_mark(&mut self, index: usize) {
        if !self.selected_indices.remove(&index) {
            self.selected_indices.insert(index);
        }
    }

    /// Toggle mark at focus (caller decides whether the row is markable), then move down.
    pub fn toggle_mark_and_advance(&mut self, markable: bool, len: usize) {
        let Some(focus) = self.focused_index else {
            return;
        };
        if markable && focus < len {
            self.toggle_mark(focus);
        }
        if len > 0 {
            self.set_focus((focus + 1).min(len - 1), len);
        }
    }

    /// Clear marks and place the cursor on a single index.
    pub fn set_single(&mut self, index: usize, len: usize) {
        if index < len {
            self.anchor_index = Some(index);
            self.focused_index = Some(index);
            self.selected_indices.clear();
        }
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.anchor_index = None;
        self.focused_index = None;
        self.selected_indices.clear();
    }
}

pub fn snapshot_selection(selection: &SelectionModel, entries: &[Entry]) -> SelectionSnapshot {
    SelectionSnapshot {
        cursor_key: selection
            .focus_index()
            .and_then(|index| entries.get(index))
            .map(Entry::key),
        selected_keys: selection
            .selected_indices()
            .filter_map(|index| entries.get(index))
            .map(Entry::key)
            .collect(),
        cursor_index: selection.focus_index(),
    }
}

pub fn apply_selection(entries: &[Entry], intent: &SelectionIntent) -> SelectionModel {
    if entries.is_empty() {
        return SelectionModel::default();
    }

    let selected = intent
        .snapshot
        .selected_keys
        .iter()
        .filter_map(|key| find_index(entries, key))
        .collect::<BTreeSet<_>>();

    let cursor = intent
        .preferred_cursor
        .as_ref()
        .and_then(|key| find_index(entries, key))
        .or_else(|| {
            intent
                .snapshot
                .cursor_key
                .as_ref()
                .and_then(|key| find_index(entries, key))
        })
        .or_else(|| fallback_index(entries, intent.fallback, intent.snapshot.cursor_index))
        .or_else(|| selected.iter().next().copied());

    SelectionModel::new(selected, cursor, cursor)
}

fn find_index(entries: &[Entry], key: &EntryKey) -> Option<usize> {
    entries.iter().position(|entry| entry.key() == *key)
}

fn fallback_index(
    entries: &[Entry],
    fallback: SelectionFallback,
    previous_index: Option<usize>,
) -> Option<usize> {
    match fallback {
        SelectionFallback::None => None,
        SelectionFallback::ParentOrFirst => entries
            .iter()
            .position(|entry| entry.is_parent_link)
            .or(Some(0)),
        SelectionFallback::First => Some(0),
        SelectionFallback::NeighborOrClamp => {
            previous_index.map(|index| index.min(entries.len() - 1))
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/domain_selection_tests.rs"]
mod tests;
