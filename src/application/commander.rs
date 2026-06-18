use std::{fs, path::PathBuf};

use anyhow::{bail, Context, Result};
use rust_i18n::t;

use crate::{
    application::{app_state::AppState, commands::ViewUpdate, ActivePanel},
    config::{ArchiveConfig, PanelSettings},
    domain::{
        operation::{ArchiveSourceRequest, FileOperationKind, FileOperationRequest},
        sorting::{SortColumn, SortDirection},
        Entry, Panel, PanelLocation,
    },
    fs::reader::{read_entries, rename_path},
    platform,
    presentation,
};

pub struct Commander {
    state: AppState,
    panel_settings: PanelSettings,
}

impl Commander {
    pub fn new(
        left_initial_path: PathBuf,
        right_initial_path: PathBuf,
        _archive_config: ArchiveConfig,
        panel_settings: PanelSettings,
    ) -> Result<Self> {
        let left = Panel::new(
            PanelLocation::filesystem(left_initial_path.clone()),
            read_entries(&left_initial_path, panel_settings.show_hidden_files)?,
            panel_settings.folders_first,
        );
        let right = Panel::new(
            PanelLocation::filesystem(right_initial_path.clone()),
            read_entries(&right_initial_path, panel_settings.show_hidden_files)?,
            panel_settings.folders_first,
        );
        let roots = platform::available_roots();
        Ok(Self {
            state: AppState::new(left, right, roots, presentation::ready_status()),
            panel_settings,
        })
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }

    pub fn panel_directory(&self, panel: ActivePanel) -> PathBuf {
        self.state.panel(panel).location.host_directory()
    }

    pub fn apply_archive_config(&mut self, archive_config: ArchiveConfig) -> ViewUpdate {
        let _ = archive_config;
        self.state.status = t!("status.archive_settings_updated").into_owned();
        ViewUpdate::status()
    }

    pub fn apply_panel_settings(&mut self, panel_settings: PanelSettings) -> Result<ViewUpdate> {
        self.panel_settings = panel_settings.clone();
        self.state
            .left
            .set_folders_first(self.panel_settings.folders_first);
        self.state
            .right
            .set_folders_first(self.panel_settings.folders_first);
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

    pub fn select_indices(
        &mut self,
        panel: ActivePanel,
        indices: impl IntoIterator<Item = usize>,
    ) -> ViewUpdate {
        self.state.active_panel = panel;
        self.state
            .panel_mut(panel)
            .set_selection_from_indices(indices);
        ViewUpdate::selection(panel)
    }

    pub fn select_single(&mut self, panel: ActivePanel, index: usize) -> ViewUpdate {
        self.state.active_panel = panel;
        self.state.panel_mut(panel).select_single(index);
        ViewUpdate::selection(panel)
    }

    pub fn navigate_to_loaded(
        &mut self,
        panel: ActivePanel,
        next_location: PanelLocation,
        entries: Vec<Entry>,
        status: String,
    ) -> ViewUpdate {
        self.state.active_panel = panel;
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
    ) -> ViewUpdate {
        self.state.panel_mut(panel).replace_entries(entries);
        self.state.status = status;
        ViewUpdate::panel_entries(panel)
    }

    pub fn sort_panel(
        &mut self,
        panel: ActivePanel,
        column: SortColumn,
        direction: SortDirection,
    ) -> ViewUpdate {
        self.state.active_panel = panel;
        self.state
            .panel_mut(panel)
            .set_sort_state(column, direction);
        self.state.status = t!(
            "status.sorted_panel",
            panel = presentation::panel_label(panel),
            column = presentation::sort_column_label(column)
        )
        .into_owned();
        ViewUpdate::panel_entries(panel)
    }

    pub fn rename_active(&mut self, new_name: &str) -> Result<ViewUpdate> {
        let panel = self.state.active_panel;
        let old_name = self
            .state
            .panel(panel)
            .selected_entry()
            .map(|entry| entry.name.clone())
            .context("No entry selected for rename")?;
        let (source, target) = self.state.panel(panel).rename_target(new_name.trim())?;

        if source == target {
            self.state.status = t!("status.rename_skipped").into_owned();
            return Ok(ViewUpdate::status());
        }

        rename_path(&source, &target)?;
        self.state
            .panel_mut(panel)
            .update_history_after_rename(&old_name, new_name.trim());
        self.state.status = t!("status.renamed", path = target.display().to_string()).into_owned();

        Ok(ViewUpdate::status())
    }

    pub fn create_directory_in_active(&mut self, name: &str) -> Result<ViewUpdate> {
        let panel = self.state.active_panel;
        let trimmed = name.trim();

        if trimmed.is_empty() {
            bail!("The directory name must not be empty");
        }

        if trimmed.contains('/') || trimmed.contains('\\') {
            bail!("The directory name must not contain path separators");
        }

        let Some(base_path) = self.state.panel(panel).location.filesystem_path() else {
            bail!("Directories can only be created in the real filesystem");
        };
        let target = base_path.join(trimmed);
        if target.exists() {
            bail!("An entry with this name already exists");
        }

        fs::create_dir(&target)
            .with_context(|| format!("Could not create directory {}", target.display()))?;

        self.state.status = t!(
            "status.created_directory",
            path = target.display().to_string()
        )
        .into_owned();

        Ok(ViewUpdate::status())
    }

    pub fn operation_request(&self, kind: FileOperationKind) -> Result<FileOperationRequest> {
        let source_panel = self.state.active_panel();
        let target_panel = self.state.inactive_panel();

        if source_panel.location.filesystem_path().is_none()
            && !matches!(kind, FileOperationKind::Copy)
        {
            bail!("Archive sources currently support copy only");
        }

        let selected_items = source_panel
            .selected_items()
            .into_iter()
            .collect::<Vec<_>>();

        if selected_items.is_empty() {
            bail!("No entries selected for this file operation");
        }

        let target_directory = match kind {
            FileOperationKind::Delete => None,
            FileOperationKind::Copy | FileOperationKind::Move => {
                Some(target_panel.location.host_directory())
            }
        };

        let archive_source = match &source_panel.location {
            PanelLocation::Archive(view) => {
                if target_panel.location.filesystem_path().is_none() {
                    bail!("Archive items can only be copied to a real filesystem directory");
                }
                Some(ArchiveSourceRequest {
                    session: view.session.clone(),
                    entry_paths: selected_items
                        .iter()
                        .filter_map(|item| item.archive_path.clone())
                        .collect(),
                })
            }
            PanelLocation::Filesystem(_) => None,
        };

        Ok(FileOperationRequest {
            kind,
            sources: selected_items.into_iter().map(|item| item.path).collect(),
            target_directory,
            archive_source,
        })
    }

    pub fn set_status(&mut self, status: impl Into<String>) -> ViewUpdate {
        self.state.status = status.into();
        ViewUpdate::status()
    }
}
