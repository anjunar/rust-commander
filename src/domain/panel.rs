use std::{
    collections::{BTreeSet, HashMap},
    path::{Path, PathBuf},
};

use anyhow::{bail, Result};

use crate::domain::{
    entry::Entry,
    entry_key::EntryKey,
    folder_sort::normalize_folder_path,
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
    pub folders_first: bool,
    remembered_cursor_by_location: HashMap<String, EntryKey>,
    pending_selection_intent: Option<SelectionIntent>,
    /// Directory content sizes measured via Space (key → total bytes).
    calculated_dir_sizes: HashMap<EntryKey, u64>,
    /// Session-only overrides of remembered folder sort (normalized path → sort).
    folder_sort_session_overrides: HashMap<PathBuf, SortState>,
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
            calculated_dir_sizes: HashMap::new(),
            folder_sort_session_overrides: HashMap::new(),
        }
    }

    /// Remember a manual sort choice for this folder until the process ends.
    pub fn note_folder_sort_session_override(&mut self, path: &Path, sort: SortState) {
        self.folder_sort_session_overrides
            .insert(normalize_folder_path(path), sort);
    }

    pub fn clear_folder_sort_session_override(&mut self, path: &Path) {
        self.folder_sort_session_overrides
            .remove(&normalize_folder_path(path));
    }

    pub fn folder_sort_session_override(&self, path: &Path) -> Option<SortState> {
        self.folder_sort_session_overrides
            .get(&normalize_folder_path(path))
            .copied()
    }

    pub fn replace_entries(&mut self, entries: Vec<Entry>) {
        // Entries from directory loads are pre-sorted on the worker thread.
        let intent = self
            .pending_selection_intent
            .take()
            .unwrap_or_else(|| SelectionIntent::preserve(self.selection_snapshot()));
        self.selection = apply_selection(&entries, &intent);
        self.entries = entries;
        self.calculated_dir_sizes
            .retain(|key, _| self.entries.iter().any(|entry| entry.key() == *key));
        for entry in &mut self.entries {
            if let Some(bytes) = self.calculated_dir_sizes.get(&entry.key()) {
                entry.size_bytes = *bytes;
            }
        }
    }

    pub fn navigate_to(&mut self, next_location: PanelLocation, entries: Vec<Entry>) {
        // Entries from directory loads are pre-sorted on the worker thread.
        self.remember_current_cursor();
        let intent = self.pending_selection_intent.take().unwrap_or_else(|| {
            let preferred_cursor = self
                .location
                .entry_key_when_leaving_to(&next_location)
                .or_else(|| self.remembered_cursor_for(&next_location));
            SelectionIntent::for_navigation(preferred_cursor)
        });
        self.location = next_location;
        self.selection = apply_selection(&entries, &intent);
        self.entries = entries;
        self.calculated_dir_sizes.clear();
    }

    pub fn selected_entry(&self) -> Option<&Entry> {
        self.selection
            .primary_index()
            .and_then(|selected| self.entries.get(selected))
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
        let marked = self
            .selection
            .selected_indices()
            .filter_map(|index| self.entry_as_selected(index))
            .collect::<Vec<_>>();
        if !marked.is_empty() {
            return marked;
        }
        self.selection
            .focus_index()
            .and_then(|index| self.entry_as_selected(index))
            .into_iter()
            .collect()
    }

    /// Number of marked files (excludes the bare cursor).
    #[allow(dead_code)]
    pub fn marked_count(&self) -> usize {
        self.selection
            .selected_indices()
            .filter(|index| {
                self.entries
                    .get(*index)
                    .is_some_and(|entry| !entry.is_parent_link)
            })
            .count()
    }

    #[allow(dead_code)]
    pub fn set_marks(&mut self, indices: impl IntoIterator<Item = usize>) {
        self.selection.set_marks(indices, self.entries.len());
    }

    /// Move the cursor without clearing marks.
    pub fn set_cursor(&mut self, index: usize) {
        self.selection.set_focus(index, self.entries.len());
    }

    /// Clear marks and place the cursor on a single index.
    pub fn select_single(&mut self, index: usize) {
        self.selection.set_single(index, self.entries.len());
    }

    /// Toggle mark under the cursor (skips `..`), then move the cursor down.
    pub fn toggle_mark(&mut self) {
        let Some(focus) = self.selection.focus_index() else {
            return;
        };
        let markable = self
            .entries
            .get(focus)
            .is_some_and(|entry| !entry.is_parent_link);
        self.selection
            .toggle_mark_and_advance(markable, self.entries.len());
    }

    /// Toggle mark under the cursor without moving (skips `..`).
    pub fn toggle_mark_in_place(&mut self) {
        let Some(focus) = self.selection.focus_index() else {
            return;
        };
        if self
            .entries
            .get(focus)
            .is_some_and(|entry| !entry.is_parent_link)
        {
            self.selection.toggle_mark(focus);
        }
    }

    pub fn calculated_dir_size(&self, key: &EntryKey) -> Option<u64> {
        self.calculated_dir_sizes.get(key).copied()
    }

    /// Store a measured directory size. Returns the row index when the entry is still present.
    pub fn set_calculated_dir_size(&mut self, key: EntryKey, bytes: u64) -> Option<usize> {
        self.calculated_dir_sizes.insert(key.clone(), bytes);
        let index = self.entries.iter().position(|entry| entry.key() == key)?;
        self.entries[index].size_bytes = bytes;
        Some(index)
    }

    pub fn size_label_for(&self, entry: &Entry) -> String {
        crate::presentation::entry_size_label_with_calculated(
            entry,
            self.calculated_dir_size(&entry.key()),
        )
    }

    /// Mark every entry except `..`; cursor stays put.
    pub fn mark_all(&mut self) {
        let marks = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| !entry.is_parent_link)
            .map(|(index, _)| index)
            .collect::<BTreeSet<_>>();
        self.selection.set_marks(marks, self.entries.len());
    }

    /// Invert marks on every entry except `..`; cursor stays put.
    pub fn invert_marks(&mut self) {
        let marks = self
            .entries
            .iter()
            .enumerate()
            .filter(|(index, entry)| {
                !entry.is_parent_link && !self.selection.is_marked(*index)
            })
            .map(|(index, _)| index)
            .collect::<BTreeSet<_>>();
        self.selection.set_marks(marks, self.entries.len());
    }

    pub fn cursor_index(&self) -> Option<usize> {
        self.selection.focus_index()
    }

    /// Index of the first entry whose name matches `query` as a case-insensitive prefix.
    /// Keeps the current focus when it still matches (so a longer query can narrow in place).
    pub fn index_matching_name_prefix(&self, query: &str) -> Option<usize> {
        if query.is_empty() {
            return None;
        }
        let query = query.to_lowercase();
        if let Some(index) = self.selection.focus_index() {
            if entry_matches_name_prefix(&self.entries[index], &query) {
                return Some(index);
            }
        }
        self.entries
            .iter()
            .position(|entry| entry_matches_name_prefix(entry, &query))
    }

    pub fn selection_indices(&self) -> BTreeSet<usize> {
        self.selection.selected_indices().collect()
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

    pub fn queue_reveal_name(&mut self, name: &str) {
        let snapshot = self.selection_snapshot();
        let key = if name == ".." {
            EntryKey::ParentLink
        } else {
            match &self.location {
                PanelLocation::Remote(view) => {
                    let path = crate::remote::RemotePath::new(&view.current_path)
                        .join(name)
                        .to_string();
                    EntryKey::RemoteEntry(path)
                }
                PanelLocation::Archive(view) => {
                    let path = if view.current_path.is_empty() {
                        name.to_string()
                    } else {
                        format!("{}/{}", view.current_path.trim_end_matches('/'), name)
                    };
                    EntryKey::ArchiveEntry(path)
                }
                PanelLocation::Filesystem(_) => EntryKey::FilesystemName(name.into()),
            }
        };
        self.queue_selection_intent(SelectionIntent::reveal(key, snapshot));
    }

    pub fn cursor_name(&self) -> Option<String> {
        self.selected_entry().map(|entry| entry.name.clone())
    }

    pub fn refresh_selection_intent(&self) -> SelectionIntent {
        self.pending_selection_intent
            .clone()
            .unwrap_or_else(|| {
                let snapshot = self.selection_snapshot();
                // Empty panels (startup) have no cursor yet — land on `..` / first row
                // after the first directory load instead of leaving focus unset while
                // GTK SingleSelection still highlights a row.
                if snapshot.cursor_key.is_none() && snapshot.cursor_index.is_none() {
                    SelectionIntent::for_navigation(None)
                } else {
                    SelectionIntent::preserve(snapshot)
                }
            })
    }

    pub fn queue_delete_selection(&mut self) {
        self.queue_selection_intent(SelectionIntent::after_delete(self.selection_snapshot()));
    }

    pub fn clear_marks_keeping_cursor(&mut self) {
        let focus = self.selection.focus_index();
        self.selection = SelectionModel::new(BTreeSet::new(), focus, focus);
    }

    pub fn queue_clear_marks(&mut self) {
        self.clear_marks_keeping_cursor();
        self.queue_selection_intent(SelectionIntent::clear_marks(self.selection_snapshot()));
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

    fn entry_as_selected(&self, index: usize) -> Option<SelectedEntry> {
        let entry = self.entries.get(index)?;
        if entry.is_parent_link {
            return None;
        }
        Some(SelectedEntry {
            filesystem_path: self.location.entry_filesystem_path(entry),
            archive_path: entry.archive_path.clone(),
            remote_path: entry.remote_path.clone(),
            is_dir: entry.is_dir,
            is_parent_link: false,
            display_name: entry.name.clone(),
            display_path: entry.display_path(&self.location),
        })
    }
}

fn entry_matches_name_prefix(entry: &Entry, query: &str) -> bool {
    if entry.is_parent_link {
        return query == "." || query == "..";
    }
    entry.name.to_lowercase().starts_with(query)
}

#[cfg(test)]
#[path = "../../tests/unit/domain_panel_tests.rs"]
mod tests;
