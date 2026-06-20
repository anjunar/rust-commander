use std::{
    collections::{BTreeSet, HashMap},
    path::{Path, PathBuf},
};

use anyhow::{bail, Result};

use crate::domain::{
    entry::Entry,
    entry_key::EntryKey,
    panel_location::PanelLocation,
    selection::{
        apply_selection, snapshot_selection, SelectionIntent, SelectionModel, SelectionSnapshot,
    },
    sorting::{sort_entries, SortColumn, SortDirection, SortState},
};

#[derive(Clone, Debug)]
pub struct SelectedEntry {
    pub filesystem_path: Option<PathBuf>,
    pub archive_path: Option<String>,
    pub remote_path: Option<String>,
    pub is_dir: bool,
    pub is_parent_link: bool,
    pub display_name: String,
    pub display_path: String,
}

#[derive(Clone, Debug)]
pub struct Panel {
    pub location: PanelLocation,
    pub entries: Vec<Entry>,
    pub selection: SelectionModel,
    pub sort_state: SortState,
    folders_first: bool,
    remembered_cursor_by_location: HashMap<String, EntryKey>,
    pending_selection_intent: Option<SelectionIntent>,
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
            remembered_cursor_by_location: HashMap::new(),
            pending_selection_intent: None,
        }
    }

    pub fn replace_entries(&mut self, mut entries: Vec<Entry>) {
        sort_entries(&mut entries, self.sort_state, self.folders_first);
        let intent = self
            .pending_selection_intent
            .take()
            .unwrap_or_else(|| SelectionIntent::preserve(self.selection_snapshot()));
        self.selection = apply_selection(&entries, &intent);
        self.entries = entries;
    }

    pub fn navigate_to(&mut self, next_location: PanelLocation, mut entries: Vec<Entry>) {
        self.remember_current_cursor();
        let intent = SelectionIntent::for_navigation(self.remembered_cursor_for(&next_location));
        self.location = next_location;
        sort_entries(&mut entries, self.sort_state, self.folders_first);
        self.selection = apply_selection(&entries, &intent);
        self.entries = entries;
        self.pending_selection_intent = None;
    }

    pub fn selected_entry(&self) -> Option<&Entry> {
        self.selection
            .primary_index()
            .and_then(|selected| self.entries.get(selected))
    }

    pub fn selected_path(&self) -> Option<PathBuf> {
        self.selected_entry()
            .and_then(|entry| self.location.entry_filesystem_path(entry))
    }

    pub fn selected_item(&self) -> Option<SelectedEntry> {
        let entry = self.selected_entry()?;
        Some(SelectedEntry {
            filesystem_path: self.location.entry_filesystem_path(entry),
            archive_path: entry.archive_path.clone(),
            remote_path: entry.remote_path.clone(),
            is_dir: entry.is_dir,
            is_parent_link: entry.is_parent_link,
            display_name: entry.name.clone(),
            display_path: entry.display_path(&self.location),
        })
    }

    pub fn selected_items(&self) -> Vec<SelectedEntry> {
        self.selection
            .selected_indices()
            .filter_map(|index| self.entries.get(index))
            .filter(|entry| !entry.is_parent_link)
            .map(|entry| SelectedEntry {
                filesystem_path: self.location.entry_filesystem_path(entry),
                archive_path: entry.archive_path.clone(),
                remote_path: entry.remote_path.clone(),
                is_dir: entry.is_dir,
                is_parent_link: false,
                display_name: entry.name.clone(),
                display_path: entry.display_path(&self.location),
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
        let preserved_selection = self.selection_snapshot();
        self.sort_state = self.sort_state.toggled_for(column);
        sort_entries(&mut self.entries, self.sort_state, self.folders_first);
        self.selection = apply_selection(
            &self.entries,
            &SelectionIntent::preserve(preserved_selection),
        );
    }

    pub fn set_sort_state(&mut self, column: SortColumn, direction: SortDirection) {
        let preserved_selection = self.selection_snapshot();
        self.sort_state = SortState { column, direction };
        sort_entries(&mut self.entries, self.sort_state, self.folders_first);
        self.selection = apply_selection(
            &self.entries,
            &SelectionIntent::preserve(preserved_selection),
        );
    }

    pub fn set_folders_first(&mut self, folders_first: bool) {
        if self.folders_first == folders_first {
            return;
        }

        let preserved_selection = self.selection_snapshot();
        self.folders_first = folders_first;
        sort_entries(&mut self.entries, self.sort_state, self.folders_first);
        self.selection = apply_selection(
            &self.entries,
            &SelectionIntent::preserve(preserved_selection),
        );
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

    pub fn queue_selection_intent(&mut self, intent: SelectionIntent) {
        self.pending_selection_intent = Some(intent);
    }

    pub fn refresh_selection_intent(&self) -> SelectionIntent {
        self.pending_selection_intent
            .clone()
            .unwrap_or_else(|| SelectionIntent::preserve(self.selection_snapshot()))
    }

    pub fn queue_delete_selection(&mut self) {
        self.queue_selection_intent(SelectionIntent::after_delete(self.selection_snapshot()));
    }

    pub fn rename_selected_entry(&mut self, new_name: &str) -> Result<()> {
        let Some(selected_index) = self.selection.primary_index() else {
            return Ok(());
        };
        let snapshot = self.selection_snapshot();
        let Some(entry) = self.entries.get_mut(selected_index) else {
            return Ok(());
        };
        entry.name = new_name.into();
        sort_entries(&mut self.entries, self.sort_state, self.folders_first);
        self.selection = apply_selection(
            &self.entries,
            &SelectionIntent::reveal(EntryKey::FilesystemName(new_name.into()), snapshot),
        );
        self.remembered_cursor_by_location.insert(
            self.location.history_key(),
            EntryKey::FilesystemName(new_name.into()),
        );
        Ok(())
    }

    pub fn apply_filesystem_entry_change(
        &mut self,
        path: &Path,
        next_entry: Option<Entry>,
    ) -> bool {
        let Some(panel_path) = self.location.filesystem_path() else {
            return false;
        };
        if path.parent() != Some(panel_path) {
            return false;
        }

        let Some(name) = path
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
        else {
            return false;
        };
        let changed_key = EntryKey::FilesystemName(name.clone().into());
        let snapshot = self.selection_snapshot();
        let removed_selected = snapshot.cursor_key.as_ref() == Some(&changed_key)
            || snapshot.selected_keys.contains(&changed_key);

        let mut changed = false;
        self.entries.retain(|entry| {
            let keep = entry.is_parent_link || entry.name != name;
            if !keep {
                changed = true;
            }
            keep
        });

        if let Some(entry) = next_entry {
            self.entries.push(entry);
            changed = true;
        }

        if !changed {
            return false;
        }

        sort_entries(&mut self.entries, self.sort_state, self.folders_first);
        let intent = if removed_selected {
            SelectionIntent::after_delete(snapshot)
        } else {
            SelectionIntent::preserve(snapshot)
        };
        self.selection = apply_selection(&self.entries, &intent);
        true
    }

    fn remember_current_cursor(&mut self) {
        let Some(key) = self.selection_snapshot().cursor_key else {
            return;
        };
        self.remembered_cursor_by_location
            .insert(self.location.history_key(), key);
    }

    fn remembered_cursor_for(&self, location: &PanelLocation) -> Option<EntryKey> {
        self.remembered_cursor_by_location
            .get(&location.history_key())
            .cloned()
    }

    fn selection_snapshot(&self) -> SelectionSnapshot {
        snapshot_selection(&self.selection, &self.entries)
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, time::SystemTime};

    use super::Panel;
    use crate::domain::{
        entry::{Entry, EntryKind},
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
        assert_eq!(
            panel.selection.anchor_index(),
            panel.selection.focus_index()
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
            true,
        );

        panel.select_single(1);
        panel.set_sort_state(SortColumn::Size, SortDirection::Ascending);

        assert_eq!(panel.selected_entry().unwrap().name, "b.txt");
    }

    #[test]
    fn replace_entries_does_not_select_first_row_when_selection_disappears() {
        let mut panel = Panel::new(
            PanelLocation::filesystem(std::path::PathBuf::from("/tmp")),
            vec![entry("alpha"), entry("beta"), entry("gamma")],
            true,
        );

        panel.select_single(1);
        panel.replace_entries(vec![entry("alpha"), entry("gamma")]);

        assert!(panel.selection_indices().is_empty());
        assert!(panel.selected_entry().is_none());
        assert_eq!(panel.selection.focus_index(), None);
    }

    #[test]
    fn navigate_to_prefers_parent_when_no_history_exists() {
        let mut panel = Panel::new(
            PanelLocation::filesystem(std::path::PathBuf::from("/tmp")),
            vec![entry("alpha")],
            true,
        );

        panel.navigate_to(
            PanelLocation::filesystem(std::path::PathBuf::from("/tmp/child")),
            vec![Entry::parent_link(), entry("beta")],
        );

        assert_eq!(panel.selected_entry().unwrap().name, "..");
    }

    #[test]
    fn rename_selected_entry_reveals_new_name_after_sort() {
        let mut panel = Panel::new(
            PanelLocation::filesystem(std::path::PathBuf::from("/tmp")),
            vec![entry("b.txt"), entry("c.txt")],
            true,
        );

        panel.select_single(1);
        panel.rename_selected_entry("a.txt").unwrap();

        assert_eq!(panel.selected_entry().unwrap().name, "a.txt");
        assert_eq!(panel.selection.focus_index(), Some(0));
    }

    #[test]
    fn navigate_back_restores_remembered_child_directory() {
        let mut panel = Panel::new(
            PanelLocation::filesystem(std::path::PathBuf::from("/tmp")),
            vec![entry("alpha"), dir_entry("child")],
            true,
        );

        panel.select_single(0);
        panel.navigate_to(
            PanelLocation::filesystem(std::path::PathBuf::from("/tmp/child")),
            vec![Entry::parent_link(), entry("nested")],
        );
        panel.navigate_to(
            PanelLocation::filesystem(std::path::PathBuf::from("/tmp")),
            vec![entry("alpha"), dir_entry("child"), entry("zeta")],
        );

        assert_eq!(panel.selected_entry().unwrap().name, "child");
    }

    #[test]
    fn queued_delete_selection_clamps_to_parent_link_when_last_item_disappears() {
        let mut panel = Panel::new(
            PanelLocation::filesystem(std::path::PathBuf::from("/tmp/child")),
            vec![Entry::parent_link(), entry("last.txt")],
            true,
        );

        panel.select_single(1);
        panel.queue_delete_selection();
        panel.replace_entries(vec![Entry::parent_link()]);

        assert_eq!(panel.selected_entry().unwrap().name, "..");
        assert_eq!(panel.selection.focus_index(), Some(0));
    }

    #[test]
    fn queued_delete_selection_allows_empty_result() {
        let mut panel = Panel::new(
            PanelLocation::filesystem(std::path::PathBuf::from("/tmp")),
            vec![entry("only.txt")],
            true,
        );

        panel.select_single(0);
        panel.queue_delete_selection();
        panel.replace_entries(vec![]);

        assert!(panel.selected_entry().is_none());
        assert!(panel.selection_indices().is_empty());
    }

    fn entry(name: &str) -> Entry {
        sized_entry(name, 1)
    }

    fn dir_entry(name: &str) -> Entry {
        Entry {
            name: name.into(),
            archive_path: None,
            remote_path: None,
            kind: EntryKind::Directory,
            is_dir: true,
            size_bytes: 0,
            modified_at: Some(SystemTime::now()),
            attributes: String::new(),
            is_parent_link: false,
        }
    }

    fn sized_entry(name: &str, size_bytes: u64) -> Entry {
        Entry {
            name: name.into(),
            archive_path: None,
            remote_path: None,
            kind: EntryKind::File,
            is_dir: false,
            size_bytes,
            modified_at: Some(SystemTime::now()),
            attributes: String::new(),
            is_parent_link: false,
        }
    }

    #[test]
    fn apply_filesystem_entry_change_updates_existing_entry() {
        let mut panel = Panel::new(
            PanelLocation::filesystem(std::path::PathBuf::from("/tmp")),
            vec![entry("alpha.txt")],
            true,
        );

        let changed = panel.apply_filesystem_entry_change(
            std::path::Path::new("/tmp/alpha.txt"),
            Some(Entry {
                name: "alpha.txt".into(),
                archive_path: None,
                remote_path: None,
                kind: EntryKind::File,
                is_dir: false,
                size_bytes: 99,
                modified_at: Some(SystemTime::now()),
                attributes: String::new(),
                is_parent_link: false,
            }),
        );

        assert!(changed);
        assert_eq!(panel.entries.len(), 1);
        assert_eq!(panel.entries[0].size_bytes, 99);
    }

    #[test]
    fn apply_filesystem_entry_change_removes_missing_entry() {
        let mut panel = Panel::new(
            PanelLocation::filesystem(std::path::PathBuf::from("/tmp")),
            vec![entry("alpha.txt"), entry("beta.txt")],
            true,
        );

        let changed =
            panel.apply_filesystem_entry_change(std::path::Path::new("/tmp/alpha.txt"), None);

        assert!(changed);
        assert_eq!(
            panel
                .entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            vec!["beta.txt"]
        );
    }
}
