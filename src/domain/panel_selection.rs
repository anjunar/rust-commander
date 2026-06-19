use std::collections::BTreeSet;

use crate::domain::{entry::Entry, entry_key::EntryKey};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PanelSelection {
    pub cursor: Option<EntryKey>,
    pub selected: BTreeSet<EntryKey>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionFallback {
    PreserveOnly,
    PreferParentOrFirst,
}

impl PanelSelection {
    pub fn single(key: Option<EntryKey>) -> Self {
        let selected = key.iter().cloned().collect();
        Self {
            cursor: key,
            selected,
        }
    }
}

pub fn restore_panel_selection(
    previous: &PanelSelection,
    history_cursor: Option<&EntryKey>,
    next_entries: &[Entry],
    fallback: SelectionFallback,
) -> PanelSelection {
    let mut selected = previous
        .selected
        .iter()
        .filter(|key| contains_entry(next_entries, key))
        .cloned()
        .collect::<BTreeSet<_>>();

    let cursor = previous
        .cursor
        .as_ref()
        .filter(|key| contains_entry(next_entries, key))
        .cloned()
        .or_else(|| {
            history_cursor
                .filter(|key| contains_entry(next_entries, key))
                .cloned()
        });

    let cursor = match fallback {
        SelectionFallback::PreserveOnly => cursor,
        SelectionFallback::PreferParentOrFirst => cursor
            .or_else(|| {
                contains_entry(next_entries, &EntryKey::ParentLink).then_some(EntryKey::ParentLink)
            })
            .or_else(|| next_entries.first().map(Entry::key)),
    };

    if selected.is_empty() {
        if let Some(cursor) = &cursor {
            selected.insert(cursor.clone());
        }
    }

    PanelSelection { cursor, selected }
}

fn contains_entry(entries: &[Entry], key: &EntryKey) -> bool {
    entries.iter().any(|entry| entry.key() == *key)
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use super::{restore_panel_selection, PanelSelection, SelectionFallback};
    use crate::domain::{entry::Entry, entry_key::EntryKey};

    #[test]
    fn keeps_cursor_and_selected_keys_when_entries_reordered() {
        let previous = PanelSelection {
            cursor: Some(EntryKey::FilesystemName("beta".into())),
            selected: [
                EntryKey::FilesystemName("alpha".into()),
                EntryKey::FilesystemName("beta".into()),
            ]
            .into_iter()
            .collect(),
        };

        let restored = restore_panel_selection(
            &previous,
            None,
            &[entry("beta"), entry("gamma"), entry("alpha")],
            SelectionFallback::PreserveOnly,
        );

        assert_eq!(
            restored.cursor,
            Some(EntryKey::FilesystemName("beta".into()))
        );
        assert_eq!(restored.selected.len(), 2);
        assert!(restored
            .selected
            .contains(&EntryKey::FilesystemName("alpha".into())));
        assert!(restored
            .selected
            .contains(&EntryKey::FilesystemName("beta".into())));
    }

    #[test]
    fn falls_back_to_history_cursor_when_previous_cursor_disappears() {
        let previous = PanelSelection::single(Some(EntryKey::FilesystemName("missing".into())));
        let history_cursor = EntryKey::FilesystemName("gamma".into());

        let restored = restore_panel_selection(
            &previous,
            Some(&history_cursor),
            &[entry("alpha"), entry("gamma")],
            SelectionFallback::PreserveOnly,
        );

        assert_eq!(restored.cursor, Some(history_cursor));
        assert_eq!(
            restored.selected,
            [EntryKey::FilesystemName("gamma".into())]
                .into_iter()
                .collect()
        );
    }

    #[test]
    fn falls_back_to_parent_link_before_first_row() {
        let previous = PanelSelection::default();
        let restored = restore_panel_selection(
            &previous,
            None,
            &[Entry::parent_link("Up"), entry("alpha"), entry("beta")],
            SelectionFallback::PreferParentOrFirst,
        );

        assert_eq!(restored.cursor, Some(EntryKey::ParentLink));
        assert_eq!(
            restored.selected,
            [EntryKey::ParentLink].into_iter().collect()
        );
    }

    #[test]
    fn preserve_only_leaves_selection_empty_when_cursor_disappears() {
        let previous = PanelSelection::single(Some(EntryKey::FilesystemName("gone".into())));

        let restored = restore_panel_selection(
            &previous,
            None,
            &[entry("alpha"), entry("beta")],
            SelectionFallback::PreserveOnly,
        );

        assert_eq!(restored.cursor, None);
        assert!(restored.selected.is_empty());
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
}
