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
    First,
    ParentOrFirst,
}

#[derive(Clone, Debug, Default)]
pub struct SelectionModel {
    anchor_index: Option<usize>,
    focused_index: Option<usize>,
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

    pub fn for_navigation(remembered_cursor: Option<EntryKey>) -> Self {
        Self {
            snapshot: SelectionSnapshot::default(),
            preferred_cursor: remembered_cursor,
            fallback: SelectionFallback::ParentOrFirst,
        }
    }

    pub fn first() -> Self {
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

    pub fn single(index: Option<usize>) -> Self {
        match index {
            Some(index) => {
                let mut selected_indices = BTreeSet::new();
                selected_indices.insert(index);
                Self::from_cursor(selected_indices, Some(index))
            }
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

    pub fn is_selected(&self, index: usize) -> bool {
        self.selected_indices.contains(&index)
    }

    pub fn selected_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.selected_indices.iter().copied()
    }

    pub fn set_from_indices(&mut self, indices: impl IntoIterator<Item = usize>, len: usize) {
        let selected = indices
            .into_iter()
            .filter(|index| *index < len)
            .collect::<BTreeSet<_>>();

        if selected.is_empty() {
            self.clear();
            return;
        }

        let focused = self
            .focused_index
            .filter(|index| selected.contains(index))
            .or_else(|| selected.iter().next().copied());
        let anchor = self
            .anchor_index
            .filter(|index| selected.contains(index))
            .or(focused);

        self.anchor_index = anchor;
        self.focused_index = focused;
        self.selected_indices = selected;
    }

    pub fn set_single(&mut self, index: usize, len: usize) {
        if index < len {
            self.anchor_index = Some(index);
            self.focused_index = Some(index);
            self.selected_indices.clear();
            self.selected_indices.insert(index);
        }
    }

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

    let mut selected = intent
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

    if selected.is_empty() {
        if let Some(index) = cursor {
            selected.insert(index);
        }
    }

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
        SelectionFallback::First => Some(0),
        SelectionFallback::ParentOrFirst => entries
            .iter()
            .position(|entry| entry.is_parent_link)
            .or(Some(0)),
        SelectionFallback::NeighborOrClamp => {
            previous_index.map(|index| index.min(entries.len() - 1))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, time::SystemTime};

    use super::{
        apply_selection, snapshot_selection, SelectionFallback, SelectionIntent, SelectionModel,
        SelectionSnapshot,
    };
    use crate::domain::{entry::Entry, entry_key::EntryKey};

    #[test]
    fn preserve_intent_restores_selection_when_entries_reordered() {
        let selection = SelectionModel::new(BTreeSet::from([1, 2]), Some(2), Some(2));
        let snapshot =
            snapshot_selection(&selection, &[entry("alpha"), entry("beta"), entry("gamma")]);

        let restored = apply_selection(
            &[entry("gamma"), entry("alpha"), entry("beta")],
            &SelectionIntent::preserve(snapshot),
        );

        assert_eq!(restored.selected_indices().collect::<Vec<_>>(), vec![0, 2]);
        assert_eq!(restored.focus_index(), Some(0));
    }

    #[test]
    fn delete_intent_clamps_to_neighbor() {
        let selection = SelectionModel::single(Some(2));
        let snapshot =
            snapshot_selection(&selection, &[entry("alpha"), entry("beta"), entry("gamma")]);

        let restored = apply_selection(
            &[entry("alpha"), entry("beta")],
            &SelectionIntent::after_delete(snapshot),
        );

        assert_eq!(restored.selected_indices().collect::<Vec<_>>(), vec![1]);
        assert_eq!(restored.focus_index(), Some(1));
    }

    #[test]
    fn navigation_prefers_parent_link() {
        let restored = apply_selection(
            &[Entry::parent_link("Up"), entry("alpha")],
            &SelectionIntent::for_navigation(None),
        );

        assert_eq!(restored.focus_index(), Some(0));
    }

    #[test]
    fn reveal_prefers_named_entry() {
        let restored = apply_selection(
            &[entry("alpha"), entry("renamed"), entry("gamma")],
            &SelectionIntent {
                snapshot: SelectionSnapshot::default(),
                preferred_cursor: Some(EntryKey::FilesystemName("renamed".into())),
                fallback: SelectionFallback::First,
            },
        );

        assert_eq!(restored.focus_index(), Some(1));
    }

    #[test]
    fn preserve_intent_restores_archive_entries_by_archive_path() {
        let selection = SelectionModel::single(Some(1));
        let snapshot = snapshot_selection(
            &selection,
            &[archive_entry("folder/"), archive_entry("folder/file.txt")],
        );

        let restored = apply_selection(
            &[
                archive_entry("folder/file.txt"),
                archive_entry("folder/other.txt"),
            ],
            &SelectionIntent::preserve(snapshot),
        );

        assert_eq!(restored.focus_index(), Some(0));
        assert_eq!(restored.selected_indices().collect::<Vec<_>>(), vec![0]);
    }

    fn entry(name: &str) -> Entry {
        Entry {
            name: name.into(),
            archive_path: None,
            is_dir: false,
            size_bytes: 1,
            size_label: "1 B".into(),
            type_label: "File".into(),
            modified_at: Some(SystemTime::now()),
            modified_label: String::new(),
            attributes_label: String::new(),
            is_parent_link: false,
        }
    }

    fn archive_entry(path: &str) -> Entry {
        let name = path
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(path);
        Entry {
            name: name.into(),
            archive_path: Some(path.into()),
            is_dir: path.ends_with('/'),
            size_bytes: 1,
            size_label: "1 B".into(),
            type_label: "Archive".into(),
            modified_at: Some(SystemTime::now()),
            modified_label: String::new(),
            attributes_label: String::new(),
            is_parent_link: false,
        }
    }
}
