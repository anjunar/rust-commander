use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use rust_i18n::t;

use crate::{
    application::{
        app_state::AppState, commands::ViewUpdate, ActivePanel, ArchiveExtractRequest,
        FileOperationKind, LocalOperationRequest, OperationPlan, RemoteDownloadRequest,
        RemoteUploadRequest, SessionStore,
    },
    config::{ArchiveConfig, PanelSettings},
    domain::{
        selection::SelectionIntent,
        sorting::{SortColumn, SortDirection, SortState},
        Entry, Panel, PanelLocation, RootLocation,
    },
};

pub struct Commander {
    state: AppState,
    panel_settings: PanelSettings,
}

#[derive(Clone, Debug)]
pub enum CreateDirectoryTarget {
    Filesystem(PathBuf),
    Remote { session_key: String, path: String },
}

impl Commander {
    pub fn new(
        left_initial_path: PathBuf,
        right_initial_path: PathBuf,
        panel_settings: PanelSettings,
        roots: Vec<RootLocation>,
        initial_status: String,
    ) -> Self {
        let left = Panel::new(
            PanelLocation::filesystem(left_initial_path.clone()),
            Vec::new(),
            panel_settings.folders_first,
        );
        let right = Panel::new(
            PanelLocation::filesystem(right_initial_path.clone()),
            Vec::new(),
            panel_settings.folders_first,
        );
        Self {
            state: AppState::new(left, right, roots, initial_status),
            panel_settings,
        }
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }

    pub fn panel_directory(&self, panel: ActivePanel) -> Option<PathBuf> {
        self.state.panel(panel).location.host_directory()
    }

    pub fn cursor_name(&self, panel: ActivePanel) -> Option<String> {
        self.state.panel(panel).cursor_name()
    }

    pub fn queue_reveal_name_on_panel(&mut self, panel: ActivePanel, name: &str) {
        self.state.panel_mut(panel).queue_reveal_name(name);
    }

    pub fn apply_archive_config(&mut self, archive_config: ArchiveConfig) -> ViewUpdate {
        let _ = archive_config;
        self.state.status = t!("status.archive_settings_updated").into_owned();
        ViewUpdate::status()
    }

    pub fn apply_panel_settings(&mut self, panel_settings: PanelSettings) -> Result<ViewUpdate> {
        self.panel_settings = panel_settings.clone();
        for tabs in [&mut self.state.left, &mut self.state.right] {
            for panel in tabs.iter_panels_mut() {
                panel.set_folders_first(self.panel_settings.folders_first);
            }
        }
        self.state.status = t!("status.view_refreshed").into_owned();
        Ok(ViewUpdate::both_panels())
    }

    pub fn set_active_panel(&mut self, panel: ActivePanel) -> ViewUpdate {
        self.state.active_panel = panel;
        ViewUpdate {
            active_panel: true,
            status: true,
            ..ViewUpdate::default()
        }
    }

    pub fn switch_panel(&mut self) -> ViewUpdate {
        self.state.active_panel = self.state.active_panel.other();
        ViewUpdate {
            active_panel: true,
            status: true,
            ..ViewUpdate::default()
        }
    }

    /// Exchange left and right sides (all tabs), keeping focus on the same physical panel.
    pub fn swap_panels(&mut self) -> ViewUpdate {
        std::mem::swap(&mut self.state.left, &mut self.state.right);
        ViewUpdate {
            left_entries: true,
            right_entries: true,
            roots: true,
            selection: true,
            status: true,
            active_panel: true,
        }
    }

    pub fn restore_tabs(
        &mut self,
        panel: ActivePanel,
        tabs: &[crate::config::PaneTabConfig],
        active_index: usize,
    ) {
        if tabs.is_empty() {
            return;
        }
        if tabs.len() == 1 {
            let tab_cfg = &tabs[0];
            self.state
                .panel_mut(panel)
                .set_sort_state(tab_cfg.sort_column, tab_cfg.sort_direction);
            if let Some(name) = tab_cfg.cursor.as_deref().filter(|n| !n.is_empty()) {
                self.queue_reveal_name_on_panel(panel, name);
            }
            return;
        }
        let folders_first = self.panel_settings.folders_first;
        let fallback = self.state.panel(panel).location.clone();
        let first_location = tabs[0]
            .directory
            .clone()
            .map(PanelLocation::filesystem)
            .unwrap_or_else(|| fallback.clone());
        let mut first_panel = Panel::new(first_location, Vec::new(), folders_first);
        first_panel.set_sort_state(tabs[0].sort_column, tabs[0].sort_direction);
        if let Some(name) = tabs[0].cursor.as_deref().filter(|n| !n.is_empty()) {
            first_panel.queue_reveal_name(name);
        }
        let mut rebuilt = crate::domain::PanelTabs::from_panel(first_panel);
        for tab_cfg in tabs.iter().skip(1) {
            let location = tab_cfg
                .directory
                .clone()
                .map(PanelLocation::filesystem)
                .unwrap_or_else(|| fallback.clone());
            let mut new_panel = Panel::new(location, Vec::new(), folders_first);
            new_panel.set_sort_state(tab_cfg.sort_column, tab_cfg.sort_direction);
            if let Some(name) = tab_cfg.cursor.as_deref().filter(|n| !n.is_empty()) {
                new_panel.queue_reveal_name(name);
            }
            rebuilt.open(new_panel);
        }
        let target = active_index.min(rebuilt.len().saturating_sub(1));
        let _ = rebuilt.activate(target);
        match panel {
            ActivePanel::Left => self.state.left = rebuilt,
            ActivePanel::Right => self.state.right = rebuilt,
        }
    }

    pub fn open_tab(&mut self, panel: ActivePanel) -> ViewUpdate {
        self.state.active_panel = panel;
        let folders_first = self.panel_settings.folders_first;
        let location = self.state.panel(panel).location.clone();
        let new_panel = Panel::new(location, Vec::new(), folders_first);
        self.state.tabs_mut(panel).open(new_panel);
        ViewUpdate::panel_entries(panel)
    }

    pub fn close_tab(&mut self, panel: ActivePanel) -> ViewUpdate {
        self.state.active_panel = panel;
        if !self.state.tabs_mut(panel).close_active() {
            return ViewUpdate::default();
        }
        ViewUpdate::panel_entries(panel)
    }

    pub fn activate_tab(&mut self, panel: ActivePanel, index: usize) -> ViewUpdate {
        self.state.active_panel = panel;
        if !self.state.tabs_mut(panel).activate(index) {
            return ViewUpdate {
                active_panel: true,
                status: true,
                ..ViewUpdate::default()
            };
        }
        ViewUpdate::panel_entries(panel)
    }

    pub fn next_tab(&mut self, panel: ActivePanel) -> ViewUpdate {
        self.state.active_panel = panel;
        if !self.state.tabs_mut(panel).next() {
            return ViewUpdate::default();
        }
        ViewUpdate::panel_entries(panel)
    }

    pub fn prev_tab(&mut self, panel: ActivePanel) -> ViewUpdate {
        self.state.active_panel = panel;
        if !self.state.tabs_mut(panel).prev() {
            return ViewUpdate::default();
        }
        ViewUpdate::panel_entries(panel)
    }

    /// Move the panel cursor without clearing marks.
    /// Does not rebuild panel entries — GTK SingleSelection already owns the visual cursor.
    pub fn set_cursor(&mut self, panel: ActivePanel, index: usize) -> ViewUpdate {
        let panel_changed = self.state.active_panel != panel;
        self.state.active_panel = panel;
        self.state.panel_mut(panel).set_cursor(index);
        ViewUpdate {
            selection: true,
            status: true,
            active_panel: panel_changed,
            ..ViewUpdate::default()
        }
    }

    /// Clear marks and place the cursor on a single index.
    pub fn select_single(&mut self, panel: ActivePanel, index: usize) -> ViewUpdate {
        self.state.active_panel = panel;
        self.state.panel_mut(panel).select_single(index);
        ViewUpdate::selection(panel)
    }

    /// Toggle mark under the cursor (skips `..`), then move the cursor down.
    pub fn toggle_mark(&mut self, panel: ActivePanel) -> ViewUpdate {
        self.state.active_panel = panel;
        self.state.panel_mut(panel).toggle_mark();
        ViewUpdate::selection(panel)
    }

    /// Toggle mark under the cursor without moving (skips `..`).
    pub fn toggle_mark_in_place(&mut self, panel: ActivePanel) -> ViewUpdate {
        self.state.active_panel = panel;
        self.state.panel_mut(panel).toggle_mark_in_place();
        ViewUpdate::selection(panel)
    }

    pub fn set_calculated_dir_size(
        &mut self,
        panel: ActivePanel,
        key: crate::domain::EntryKey,
        bytes: u64,
    ) -> (ViewUpdate, Option<usize>) {
        let index = self
            .state
            .panel_mut(panel)
            .set_calculated_dir_size(key, bytes);
        (ViewUpdate::panel_entries(panel), index)
    }

    /// Mark all entries in the panel except `..`.
    pub fn mark_all(&mut self, panel: ActivePanel) -> ViewUpdate {
        self.state.active_panel = panel;
        self.state.panel_mut(panel).mark_all();
        ViewUpdate::selection(panel)
    }

    /// Invert marks in the panel (skips `..`).
    pub fn invert_marks(&mut self, panel: ActivePanel) -> ViewUpdate {
        self.state.active_panel = panel;
        self.state.panel_mut(panel).invert_marks();
        ViewUpdate::selection(panel)
    }

    /// Focus the entry matching a quick-search query. Returns `true` when a match was found.
    pub fn quick_search(&mut self, panel: ActivePanel, query: &str) -> (ViewUpdate, bool) {
        if query.is_empty() {
            return (ViewUpdate::status(), false);
        }

        let Some(index) = self.state.panel(panel).index_matching_name_prefix(query) else {
            return (ViewUpdate::default(), false);
        };

        self.state.active_panel = panel;
        self.state.panel_mut(panel).set_cursor(index);
        self.state.status = t!("status.quick_search", query = query).into_owned();
        (ViewUpdate::selection(panel), true)
    }

    pub fn navigate_to_loaded(
        &mut self,
        panel: ActivePanel,
        next_location: PanelLocation,
        entries: Vec<Entry>,
        status: String,
        selection_intent: Option<SelectionIntent>,
    ) -> ViewUpdate {
        self.state.active_panel = panel;
        if let Some(intent) = selection_intent {
            self.state.panel_mut(panel).queue_selection_intent(intent);
        }
        self.state
            .panel_mut(panel)
            .navigate_to(next_location, entries);
        self.state.status = status;
        ViewUpdate::panel_entries(panel)
    }

    pub fn refresh_panel_loaded(
        &mut self,
        panel: ActivePanel,
        entries: Vec<Entry>,
        status: String,
        selection_intent: Option<SelectionIntent>,
    ) -> ViewUpdate {
        if let Some(intent) = selection_intent {
            self.state.panel_mut(panel).queue_selection_intent(intent);
        }
        self.state.panel_mut(panel).replace_entries(entries);
        self.state.status = status;
        ViewUpdate::panel_entries(panel)
    }

    pub fn sort_panel(
        &mut self,
        panel: ActivePanel,
        column: SortColumn,
        direction: SortDirection,
        status: String,
    ) -> ViewUpdate {
        self.state.active_panel = panel;
        {
            let panel_state = self.state.panel_mut(panel);
            let sort = SortState { column, direction };
            if let Some(path) = panel_state
                .location
                .filesystem_path()
                .map(Path::to_path_buf)
            {
                panel_state.note_folder_sort_session_override(&path, sort);
            }
            panel_state.set_sort_state(column, direction);
        }
        self.state.status = status;
        ViewUpdate::panel_entries(panel)
    }

    /// Remember sort for an upcoming navigation without re-sorting the current listing.
    pub fn adopt_sort_state(&mut self, panel: ActivePanel, sort_state: SortState) {
        self.state.panel_mut(panel).sort_state = sort_state;
    }

    pub fn clear_folder_sort_session_override(&mut self, panel: ActivePanel, path: &Path) {
        self.state
            .panel_mut(panel)
            .clear_folder_sort_session_override(path);
    }

    pub fn rename_active_request(&self, new_name: &str) -> Result<(PathBuf, PathBuf)> {
        let panel = self.state.active_panel;
        self.state
            .panel(panel)
            .selected_entry()
            .context("No entry selected for rename")?;
        self.state.panel(panel).rename_target(new_name.trim())
    }

    pub fn apply_active_rename(
        &mut self,
        new_name: &str,
        status: impl Into<String>,
    ) -> Result<ViewUpdate> {
        let panel = self.state.active_panel;
        let (source, target) = self.rename_active_request(new_name)?;

        if source == target {
            self.state.status = t!("status.rename_skipped").into_owned();
            return Ok(ViewUpdate::status());
        }

        self.state
            .panel_mut(panel)
            .rename_selected_entry(new_name.trim())?;
        self.state.status = status.into();

        Ok(ViewUpdate::panel_entries(panel))
    }

    pub fn create_directory_request(&self, name: &str) -> Result<CreateDirectoryTarget> {
        let panel = self.state.active_panel;
        let trimmed = name.trim();

        if trimmed.is_empty() {
            bail!("The directory name must not be empty");
        }

        if trimmed.contains('/') || trimmed.contains('\\') {
            bail!("The directory name must not contain path separators");
        }

        match &self.state.panel(panel).location {
            PanelLocation::Filesystem(base_path) => {
                let target = base_path.join(trimmed);
                if target.exists() {
                    bail!("An entry with this name already exists");
                }
                Ok(CreateDirectoryTarget::Filesystem(target))
            }
            PanelLocation::Remote(view) => {
                let path = crate::remote::RemotePath::new(&view.current_path)
                    .join(trimmed)
                    .to_string();
                Ok(CreateDirectoryTarget::Remote {
                    session_key: view.session_key.clone(),
                    path,
                })
            }
            PanelLocation::Archive(_) => {
                bail!("Directories cannot be created inside archives")
            }
        }
    }

    pub fn create_file_request(&self, name: &str) -> Result<PathBuf> {
        let panel = self.state.active_panel;
        let trimmed = name.trim();

        if trimmed.is_empty() {
            bail!("The file name must not be empty");
        }

        if trimmed.contains('/') || trimmed.contains('\\') {
            bail!("The file name must not contain path separators");
        }

        let Some(base_path) = self.state.panel(panel).location.filesystem_path() else {
            bail!("Files can only be created in the real filesystem");
        };
        let target = base_path.join(trimmed);
        if target.exists() {
            bail!("An entry with this name already exists");
        }

        Ok(target)
    }

    pub fn copy_with_rename_request(&self, new_name: &str) -> Result<(PathBuf, PathBuf)> {
        let panel = self.state.active_panel;
        let (source, target) = self.state.panel(panel).rename_target(new_name.trim())?;

        if source == target {
            bail!("Choose a different name for the copy");
        }

        if target.exists() {
            bail!("An entry with this name already exists");
        }

        Ok((source, target))
    }

    pub fn queue_reveal_name_on_active(&mut self, name: &str) {
        let panel = self.state.active_panel;
        self.queue_reveal_name_on_panel(panel, name);
    }

    pub fn operation_request(
        &self,
        kind: FileOperationKind,
        session_store: &SessionStore,
    ) -> Result<OperationPlan> {
        let source_panel = self.state.active_panel();
        let target_panel = self.state.inactive_panel();

        if source_panel.location.filesystem_path().is_none()
            && !matches!(kind, FileOperationKind::Copy)
        {
            bail!("Only copy is currently supported for non-filesystem sources");
        }

        let selected_items = source_panel
            .selected_items()
            .into_iter()
            .collect::<Vec<_>>();

        if selected_items.is_empty() {
            bail!("No entries selected for this file operation");
        }

        match &source_panel.location {
            PanelLocation::Archive(view) => {
                if target_panel.location.filesystem_path().is_none() {
                    bail!("Archive items can only be copied to a real filesystem directory");
                }
                let session = session_store
                    .archive(&view.session_key)
                    .context("Archive session not found")?;
                let target_directory = target_panel
                    .location
                    .host_directory()
                    .context("No filesystem target directory available for archive copy")?;
                Ok(OperationPlan::ArchiveExtract(ArchiveExtractRequest {
                    session,
                    entry_paths: selected_items
                        .iter()
                        .filter_map(|item| item.archive_path.clone())
                        .collect(),
                    target_directory,
                }))
            }
            PanelLocation::Remote(location) => {
                if target_panel.location.filesystem_path().is_none() {
                    bail!("Remote downloads currently require a real filesystem target");
                }
                let session = session_store
                    .remote(&location.session_key)
                    .context("Remote session not found")?;
                let target_directory = target_panel
                    .location
                    .host_directory()
                    .context("No filesystem target directory available for remote download")?;
                Ok(OperationPlan::RemoteDownload(RemoteDownloadRequest {
                    session,
                    entry_paths: selected_items
                        .iter()
                        .filter_map(|item| item.remote_path.clone())
                        .collect(),
                    target_directory,
                    force_overwrite: false,
                }))
            }
            PanelLocation::Filesystem(_) => {
                let sources = selected_items
                    .iter()
                    .filter_map(|item| item.filesystem_path.clone())
                    .collect::<Vec<_>>();

                match (&target_panel.location, &kind) {
                    (PanelLocation::Remote(location), FileOperationKind::Copy) => {
                        let session = session_store
                            .remote(&location.session_key)
                            .context("Remote session not found")?;
                        Ok(OperationPlan::RemoteUpload(RemoteUploadRequest {
                            sources,
                            session,
                            target_directory: location.current_path.clone(),
                            force_overwrite: false,
                        }))
                    }
                    (PanelLocation::Remote(_), FileOperationKind::Move) => {
                        bail!("Remote targets currently support upload by copy only");
                    }
                    _ => {
                        let target_directory = match kind {
                            FileOperationKind::Delete => None,
                            FileOperationKind::Copy | FileOperationKind::Move => {
                                target_panel.location.host_directory()
                            }
                        };
                        Ok(OperationPlan::Local(LocalOperationRequest {
                            kind,
                            sources,
                            target_directory,
                            target_path: None,
                            use_recycle_bin: false,
                        }))
                    }
                }
            }
        }
    }

    pub fn set_status(&mut self, status: impl Into<String>) -> ViewUpdate {
        self.state.status = status.into();
        ViewUpdate::status()
    }

    pub fn apply_filesystem_entry_changes(
        &mut self,
        panel: ActivePanel,
        changed_paths: &[PathBuf],
        show_hidden_files: bool,
    ) -> Result<Option<ViewUpdate>> {
        let panel_path = match self
            .state
            .panel(panel)
            .location
            .filesystem_path()
            .map(PathBuf::from)
        {
            Some(path) => path,
            None => return Ok(None),
        };

        let panel_state = self.state.panel_mut(panel);
        if panel_state.location.filesystem_path().is_none() {
            return Ok(None);
        }

        let mut changed = false;
        for path in changed_paths {
            if path.parent() != Some(panel_path.as_path()) {
                continue;
            }

            let next_entry = crate::fs::reader::read_entry(path, show_hidden_files)?;
            changed |= panel_state.apply_filesystem_entry_change(path, next_entry);
        }

        if changed {
            Ok(Some(ViewUpdate::panel_entries_without_status(panel)))
        } else {
            Ok(None)
        }
    }

    pub fn queue_selection_after_file_operation(&mut self, plan: &OperationPlan) -> ViewUpdate {
        let Some(panel) = self.source_panel_for_operation(plan) else {
            return ViewUpdate::default();
        };

        match plan.kind() {
            FileOperationKind::Delete | FileOperationKind::Move => {
                self.state.panel_mut(panel).queue_delete_selection();
                ViewUpdate::default()
            }
            FileOperationKind::Copy => {
                // Source dir is unchanged on disk, so clear marks immediately for the UI.
                self.state.panel_mut(panel).queue_clear_marks();
                ViewUpdate::selection(panel)
            }
        }
    }

    fn source_panel_for_operation(&self, plan: &OperationPlan) -> Option<ActivePanel> {
        if let Some(source_directory) = plan
            .local_sources()
            .first()
            .and_then(|source| source.parent().map(std::path::Path::to_path_buf))
        {
            for panel in [ActivePanel::Left, ActivePanel::Right] {
                let matches_source_directory = self
                    .state
                    .panel(panel)
                    .location
                    .filesystem_path()
                    .map(|path| path == source_directory.as_path())
                    .unwrap_or(false);
                if matches_source_directory {
                    return Some(panel);
                }
            }
        }

        // Archive/remote downloads have no local source paths — source is the active panel.
        if matches!(
            plan,
            OperationPlan::ArchiveExtract(_) | OperationPlan::RemoteDownload(_)
        ) {
            return Some(self.state.active_panel);
        }

        None
    }
}

#[cfg(test)]
#[path = "../../tests/unit/application_commander_tests.rs"]
mod tests;
