use std::{
    collections::{BTreeSet, HashMap, HashSet},
    path::PathBuf,
};

use anyhow::{bail, Result};

use crate::domain::{
    entry::Entry,
    panel_location::PanelLocation,
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
struct SelectedPosition {
    name: String,
    is_parent_link: bool,
}

#[derive(Clone, Debug)]
pub struct Panel {
    pub location: PanelLocation,
    pub entries: Vec<Entry>,
    pub selection: SelectionModel,
    pub sort_state: SortState,
    selected_history: HashMap<String, SelectedPosition>,
}

impl Panel {
    pub fn new(location: PanelLocation, mut entries: Vec<Entry>) -> Self {
        let sort_state = SortState::default();
        sort_entries(&mut entries, sort_state);
        let selected = (!entries.is_empty()).then_some(0);

        Self {
            location,
            entries,
            selection: SelectionModel::single(selected),
            sort_state,
            selected_history: HashMap::new(),
        }
    }

    pub fn replace_entries(&mut self, mut entries: Vec<Entry>) {
        let preserved_selection = self.preserved_selection();
        sort_entries(&mut entries, self.sort_state);
        self.entries = entries;
        self.selection = self.restore_selection(preserved_selection);
    }

    pub fn navigate_to(&mut self, next_location: PanelLocation, mut entries: Vec<Entry>) {
        self.save_selection_for_current_path();
        self.location = next_location;
        sort_entries(&mut entries, self.sort_state);
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
        sort_entries(&mut self.entries, self.sort_state);
        self.selection = self.restore_selection(preserved_selection);
    }

    pub fn set_sort_state(&mut self, column: SortColumn, direction: SortDirection) {
        let preserved_selection = self.preserved_selection();
        self.sort_state = SortState { column, direction };
        sort_entries(&mut self.entries, self.sort_state);
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
            if !saved.is_parent_link && saved.name == old_name {
                saved.name = new_name.to_string();
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
            SelectedPosition {
                name: entry.name.clone(),
                is_parent_link: entry.is_parent_link,
            },
        );
    }

    fn restore_selection_for_current_path(&self) -> Option<usize> {
        if self.entries.is_empty() {
            return None;
        }

        if let Some(saved) = self.selected_history.get(&self.location.history_key()) {
            if let Some(index) = self.entries.iter().position(|entry| {
                entry.name == saved.name && entry.is_parent_link == saved.is_parent_link
            }) {
                return Some(index);
            }
        }

        Some(0)
    }

    fn preserved_selection(&self) -> PreservedSelection {
        let focused = self
            .selection
            .focus_index()
            .and_then(|index| self.entries.get(index))
            .map(|entry| (entry.name.clone(), entry.is_parent_link));
        let anchor = self
            .selection
            .anchor_index()
            .and_then(|index| self.entries.get(index))
            .map(|entry| (entry.name.clone(), entry.is_parent_link));
        let selected = self
            .selection
            .selected_indices()
            .filter_map(|index| self.entries.get(index))
            .map(|entry| entry.name.clone())
            .collect();

        PreservedSelection {
            focused,
            anchor,
            selected,
        }
    }

    fn restore_selection(&self, preserved: PreservedSelection) -> SelectionModel {
        if self.entries.is_empty() {
            return SelectionModel::default();
        }

        let mut selected_indices = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| preserved.selected.contains(&entry.name))
            .map(|(index, _)| index)
            .collect::<BTreeSet<_>>();

        let focused_index = preserved
            .focused
            .as_ref()
            .and_then(|(name, is_parent_link)| {
                self.entries.iter().position(|entry| {
                    entry.name == *name && entry.is_parent_link == *is_parent_link
                })
            })
            .or_else(|| self.restore_selection_for_current_path())
            .or_else(|| selected_indices.iter().next().copied())
            .or(Some(0));

        let anchor_index = preserved
            .anchor
            .as_ref()
            .and_then(|(name, is_parent_link)| {
                self.entries.iter().position(|entry| {
                    entry.name == *name && entry.is_parent_link == *is_parent_link
                })
            })
            .or(focused_index);

        if selected_indices.is_empty() {
            if let Some(index) = focused_index {
                selected_indices.insert(index);
            }
        }

        SelectionModel::new(selected_indices, focused_index, anchor_index)
    }
}

#[derive(Default)]
struct PreservedSelection {
    focused: Option<(String, bool)>,
    anchor: Option<(String, bool)>,
    selected: HashSet<String>,
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
    fn replace_entries_restores_focus_anchor_and_selection_by_name() {
        let mut panel = Panel::new(
            PanelLocation::filesystem(std::path::PathBuf::from("/tmp")),
            vec![
                entry("alpha"),
                entry("beta"),
                entry("gamma"),
                entry("delta"),
            ],
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
        assert_eq!(
            panel
                .selection
                .anchor_index()
                .map(|index| panel.entries[index].name.clone()),
            Some("beta".to_string())
        );
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
