use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
};

use anyhow::{bail, Result};

use crate::domain::{
    entry::Entry,
    entry_key::EntryKey,
    panel_location::PanelLocation,
    panel_selection::{restore_panel_selection, PanelSelection},
    selection::SelectionModel,
    sorting::{sort_entries, SortColumn, SortDirection, SortState},
};

#[derive(Clone, Debug)]
pub struct SelectedEntry {
    pub path: PathBuf,
    pub archive_path: Option<String>,
    pub is_dir: bool,
    pub is_parent_link: bool,
    pub display_name: String,
}

#[derive(Clone, Debug)]
pub struct Panel {
    pub location: PanelLocation,
    pub entries: Vec<Entry>,
    pub selection: SelectionModel,
    pub sort_state: SortState,
    folders_first: bool,
    selected_history: HashMap<String, EntryKey>,
}

impl Panel {
    pub fn new(location: PanelLocation, mut entries: Vec<Entry>, folders_first: bool) -> Self {
        let sort_state = SortState::default();
        sort_entries(&mut entries, sort_state, folders_first);
        let selected = (!entries.is_empty()).then_some(0);

        Self {
            location,
            entries,
            selection: SelectionModel::single(selected),
            sort_state,
            folders_first,
            selected_history: HashMap::new(),
        }
    }

    pub fn replace_entries(&mut self, mut entries: Vec<Entry>) {
        let preserved_selection = self.preserved_selection();
        sort_entries(&mut entries, self.sort_state, self.folders_first);
        self.entries = entries;
        self.selection = self.restore_selection(preserved_selection);
    }

    pub fn navigate_to(&mut self, next_location: PanelLocation, mut entries: Vec<Entry>) {
        self.save_selection_for_current_path();
        self.location = next_location;
        sort_entries(&mut entries, self.sort_state, self.folders_first);
        self.entries = entries;
        self.selection = self.restore_selection(PreservedSelection::default());
    }

    pub fn selected_entry(&self) -> Option<&Entry> {
        self.selection
            .primary_index()
            .and_then(|selected| self.entries.get(selected))
    }

    pub fn selected_path(&self) -> Option<PathBuf> {
        self.selected_entry()
            .map(|entry| entry.full_path(&self.location))
    }

    pub fn selected_item(&self) -> Option<SelectedEntry> {
        let entry = self.selected_entry()?;
        Some(SelectedEntry {
            path: entry.full_path(&self.location),
            archive_path: entry.archive_path.clone(),
            is_dir: entry.is_dir,
            is_parent_link: entry.is_parent_link,
            display_name: entry.name.clone(),
        })
    }

    pub fn selected_items(&self) -> Vec<SelectedEntry> {
        self.selection
            .selected_indices()
            .filter_map(|index| self.entries.get(index))
            .filter(|entry| !entry.is_parent_link)
            .map(|entry| SelectedEntry {
                path: entry.full_path(&self.location),
                archive_path: entry.archive_path.clone(),
                is_dir: entry.is_dir,
                is_parent_link: false,
                display_name: entry.name.clone(),
            })
            .collect()
    }

    pub fn selected_count(&self) -> usize {
        self.selection
            .selected_indices()
            .filter(|index| {
                self.entries
                    .get(*index)
                    .map(|entry| !entry.is_parent_link)
                    .unwrap_or(false)
            })
            .count()
    }

    pub fn set_selection_from_indices(&mut self, indices: impl IntoIterator<Item = usize>) {
        self.selection.set_from_indices(indices, self.entries.len());
    }

    pub fn select_single(&mut self, index: usize) {
        self.selection.set_single(index, self.entries.len());
    }

    pub fn selection_indices(&self) -> BTreeSet<usize> {
        self.selection.selected_indices().collect()
    }

    pub fn set_sort_column(&mut self, column: SortColumn) {
        let preserved_selection = self.preserved_selection();
        self.sort_state = self.sort_state.toggled_for(column);
        sort_entries(&mut self.entries, self.sort_state, self.folders_first);
        self.selection = self.restore_selection(preserved_selection);
    }

    pub fn set_sort_state(&mut self, column: SortColumn, direction: SortDirection) {
        let preserved_selection = self.preserved_selection();
        self.sort_state = SortState { column, direction };
        sort_entries(&mut self.entries, self.sort_state, self.folders_first);
        self.selection = self.restore_selection(preserved_selection);
    }

    pub fn set_folders_first(&mut self, folders_first: bool) {
        if self.folders_first == folders_first {
            return;
        }

        let preserved_selection = self.preserved_selection();
        self.folders_first = folders_first;
        sort_entries(&mut self.entries, self.sort_state, self.folders_first);
        self.selection = self.restore_selection(preserved_selection);
    }

    pub fn rename_target(&self, new_name: &str) -> Result<(PathBuf, PathBuf)> {
        let Some(entry) = self.selected_entry() else {
            bail!("No entry selected");
        };

        if entry.is_parent_link {
            bail!("The parent entry cannot be renamed");
        }

        if new_name.is_empty() {
            bail!("The new name must not be empty");
        }

        if new_name.contains('/') || new_name.contains('\\') {
            bail!("The new name must not contain path separators");
        }

        let Some(base_path) = self.location.filesystem_path() else {
            bail!("Rename is only available in the real filesystem");
        };

        let source = base_path.join(&entry.name);
        let target = base_path.join(new_name);
        Ok((source, target))
    }

    pub fn update_history_after_rename(&mut self, old_name: &str, new_name: &str) {
        if let Some(saved) = self.selected_history.get_mut(&self.location.history_key()) {
            if let EntryKey::FilesystemName(name) = saved {
                if name == &std::ffi::OsString::from(old_name) {
                    *name = new_name.into();
                }
            }
        }
    }

    fn save_selection_for_current_path(&mut self) {
        let Some(selected) = self.selection.primary_index() else {
            return;
        };
        let Some(entry) = self.entries.get(selected) else {
            return;
        };

        self.selected_history.insert(
            self.location.history_key(),
            entry.key(),
        );
    }

    fn preserved_selection(&self) -> PreservedSelection {
        PreservedSelection {
            selection: PanelSelection {
                cursor: self
                    .selection
                    .focus_index()
                    .and_then(|index| self.entries.get(index))
                    .map(Entry::key),
                selected: self
                    .selection
                    .selected_indices()
                    .filter_map(|index| self.entries.get(index))
                    .map(Entry::key)
                    .collect(),
            },
        }
    }

    fn restore_selection(&self, preserved: PreservedSelection) -> SelectionModel {
        if self.entries.is_empty() {
            return SelectionModel::default();
        }

        let restored = restore_panel_selection(
            &preserved.selection,
            self.selected_history.get(&self.location.history_key()),
            &self.entries,
        );
        let mut selected_indices = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| restored.selected.contains(&entry.key()))
            .map(|(index, _)| index)
            .collect::<BTreeSet<_>>();
        let focused_index = restored
            .cursor
            .as_ref()
            .and_then(|key| self.entries.iter().position(|entry| entry.key() == *key))
            .or_else(|| selected_indices.iter().next().copied())
            .or(Some(0));
        if selected_indices.is_empty() {
            if let Some(index) = focused_index {
                selected_indices.insert(index);
            }
        }

        SelectionModel::from_cursor(selected_indices, focused_index)
    }
}

#[derive(Default)]
struct PreservedSelection {
    selection: PanelSelection,
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, time::SystemTime};

    use super::Panel;
    use crate::domain::{
        entry::Entry,
        selection::SelectionModel,
        sorting::{SortColumn, SortDirection},
        PanelLocation,
    };

    #[test]
    fn replace_entries_restores_focus_and_selection_by_key() {
        let mut panel = Panel::new(
            PanelLocation::filesystem(std::path::PathBuf::from("/tmp")),
            vec![
                entry("alpha"),
                entry("beta"),
                entry("gamma"),
                entry("delta"),
            ],
            true,
        );

        panel.selection = SelectionModel::new(BTreeSet::from([1, 2]), Some(2), Some(1));

        panel.replace_entries(vec![
            entry("delta"),
            entry("gamma"),
            entry("beta"),
            entry("alpha"),
        ]);

        let selected = panel
            .selection_indices()
            .into_iter()
            .map(|index| panel.entries[index].name.clone())
            .collect::<Vec<_>>();
        assert_eq!(selected, vec!["beta".to_string(), "delta".to_string()]);
        assert_eq!(panel.selected_entry().unwrap().name, "delta");
        assert_eq!(panel.selection.focus_index(), Some(2));
        assert_eq!(panel.selection.anchor_index(), panel.selection.focus_index());
    }

    #[test]
    fn sort_keeps_selected_entry_by_name() {
        let mut panel = Panel::new(
            PanelLocation::filesystem(std::path::PathBuf::from("/tmp")),
            vec![
                sized_entry("b.txt", 30),
                sized_entry("a.txt", 10),
                sized_entry("c.txt", 20),
            ],
            true,
        );

        panel.select_single(1);
        panel.set_sort_state(SortColumn::Size, SortDirection::Ascending);

        assert_eq!(panel.selected_entry().unwrap().name, "b.txt");
    }

    fn entry(name: &str) -> Entry {
        sized_entry(name, 1)
    }

    fn sized_entry(name: &str, size_bytes: u64) -> Entry {
        Entry {
            name: name.into(),
            archive_path: None,
            is_dir: false,
            size_bytes,
            size_label: format!("{size_bytes} B"),
            type_label: "File".into(),
            modified_at: Some(SystemTime::now()),
            modified_label: String::new(),
            attributes_label: String::new(),
            is_parent_link: false,
        }
    }
}
