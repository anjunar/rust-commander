use std::{fs, path::PathBuf, sync::Arc};

use anyhow::{Context, Result, bail};

use crate::{
    application::{ActivePanel, app_state::AppState, commands::ViewUpdate},
    archive::{ArchiveService, SevenZipBackend},
    config::ArchiveConfig,
    domain::{
        Entry, Panel, PanelLocation,
        operation::{FileOperationKind, FileOperationRequest},
        sorting::{SortColumn, SortDirection},
    },
    fs::reader::{read_entries, rename_path},
    platform,
};

pub struct Commander {
    state: AppState,
    archive_service: ArchiveService,
}

impl Commander {
    pub fn new(initial_path: PathBuf, archive_config: ArchiveConfig) -> Result<Self> {
        let left = Panel::new(
            PanelLocation::filesystem(initial_path.clone()),
            read_entries(&initial_path)?,
        );
        let right = Panel::new(
            PanelLocation::filesystem(initial_path.clone()),
            read_entries(&initial_path)?,
        );
        let roots = platform::available_roots();
        let archive_service = ArchiveService::new(vec![Arc::new(
            SevenZipBackend::from_optional_path(archive_config.seven_zip_path),
        )]);

        Ok(Self {
            state: AppState::new(left, right, roots),
            archive_service,
        })
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }

    pub fn archive_service(&self) -> ArchiveService {
        self.archive_service.clone()
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

    pub fn activate_index(&mut self, panel: ActivePanel, index: usize) -> Result<ViewUpdate> {
        self.select_single(panel, index);
        self.activate_selected(panel)
    }

    pub fn activate_selected(&mut self, panel: ActivePanel) -> Result<ViewUpdate> {
        self.state.active_panel = panel;
        let selected = self
            .state
            .panel(panel)
            .selected_item()
            .context("No entry selected")?;

        if selected.is_parent_link {
            return self.go_parent(panel);
        }

        let current_location = self.state.panel(panel).location.clone();

        match current_location {
            PanelLocation::Filesystem(_) if selected.is_dir => {
                let entries = read_entries(&selected.path)?;
                self.state.panel_mut(panel).navigate_to(
                    PanelLocation::filesystem(selected.path.clone()),
                    entries,
                );
                self.state.status = format!("Opened: {}", selected.path.display());
                Ok(ViewUpdate::panel_entries(panel))
            }
            PanelLocation::Filesystem(_) if self.archive_service.is_archive_path(&selected.path) => {
                let archive_location = self.archive_service.archive_location_for_path(&selected.path)?;
                let entries = self.archive_service.entries_for_location(&archive_location)?;
                self.state
                    .panel_mut(panel)
                    .navigate_to(archive_location, entries);
                self.state.status = format!("Opened archive: {}", selected.path.display());
                Ok(ViewUpdate::panel_entries(panel))
            }
            PanelLocation::Archive(view) if selected.is_dir => {
                let archive_path = selected
                    .archive_path
                    .context("Archive entry is missing its path")?;
                let next_location = PanelLocation::archive(view.session, archive_path);
                let entries = self.archive_service.entries_for_location(&next_location)?;
                self.state
                    .panel_mut(panel)
                    .navigate_to(next_location, entries);
                self.state.status = format!("Opened archive folder: {}", selected.path.display());
                Ok(ViewUpdate::panel_entries(panel))
            }
            PanelLocation::Archive(_) => {
                bail!("Opening archive files in the viewer is not wired yet")
            }
            PanelLocation::Filesystem(_) => {
                platform::open_path(&selected.path)?;
                self.state.status =
                    format!("Opened with default app: {}", selected.path.display());
                Ok(ViewUpdate::status())
            }
        }
    }

    pub fn go_parent(&mut self, panel: ActivePanel) -> Result<ViewUpdate> {
        self.state.active_panel = panel;
        let next_location = self
            .state
            .panel(panel)
            .location
            .parent()
            .context("No parent location available")?;
        let entries = self.archive_service.entries_for_location(&next_location)?;
        self.state
            .panel_mut(panel)
            .navigate_to(next_location.clone(), entries);
        self.state.status = format!("Up one level: {}", next_location.display_label());
        Ok(ViewUpdate::panel_entries(panel))
    }

    pub fn change_root(&mut self, panel: ActivePanel, index: usize) -> Result<ViewUpdate> {
        let Some(root) = self.state.roots.get(index).cloned() else {
            return Ok(ViewUpdate::default());
        };

        self.state.active_panel = panel;
        let entries = read_entries(&root.path)?;
        self.state
            .panel_mut(panel)
            .navigate_to(PanelLocation::filesystem(root.path.clone()), entries);
        self.state.status = format!(
            "Switched {} panel to {}",
            panel.label(),
            root.path.display()
        );
        Ok(ViewUpdate::panel_entries(panel))
    }

    pub fn navigate_to_loaded(
        &mut self,
        panel: ActivePanel,
        next_location: PanelLocation,
        entries: Vec<Entry>,
        status: String,
    ) -> ViewUpdate {
        self.state.active_panel = panel;
        self.state.panel_mut(panel).navigate_to(next_location, entries);
        self.state.status = status;
        ViewUpdate::panel_entries(panel)
    }

    pub fn refresh_visible_panels(&mut self) -> Result<ViewUpdate> {
        self.state.roots = platform::available_roots();
        let left_entries = self.archive_service.entries_for_location(&self.state.left.location)?;
        let right_entries = self.archive_service.entries_for_location(&self.state.right.location)?;
        self.state.left.replace_entries(left_entries);
        self.state.right.replace_entries(right_entries);
        self.state.status = "File changes detected. View refreshed.".into();
        Ok(ViewUpdate {
            roots: true,
            ..ViewUpdate::both_panels()
        })
    }

    pub fn refresh_panels(
        &mut self,
        panels: &[ActivePanel],
        status: impl Into<String>,
    ) -> ViewUpdate {
        let mut update = ViewUpdate::default();

        for panel in panels {
            match self
                .archive_service
                .entries_for_location(&self.state.panel(*panel).location)
            {
                Ok(entries) => {
                    self.state.panel_mut(*panel).replace_entries(entries);
                    match panel {
                        ActivePanel::Left => update.left_entries = true,
                        ActivePanel::Right => update.right_entries = true,
                    }
                }
                Err(error) => {
                    self.state.status = format!("{} refresh failed: {error}", panel.label());
                }
            }
        }

        update.selection = true;
        update.status = true;
        self.state.status = status.into();
        update
    }

    pub fn refresh_with_status(&mut self, status: String) -> ViewUpdate {
        self.state.roots = platform::available_roots();

        match self.archive_service.entries_for_location(&self.state.left.location) {
            Ok(entries) => self.state.left.replace_entries(entries),
            Err(error) => self.state.status = format!("Left refresh failed: {error}"),
        }

        match self.archive_service.entries_for_location(&self.state.right.location) {
            Ok(entries) => self.state.right.replace_entries(entries),
            Err(error) => self.state.status = format!("Right refresh failed: {error}"),
        }

        self.state.status = status;
        ViewUpdate {
            roots: true,
            ..ViewUpdate::both_panels()
        }
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
        self.state.status = format!("Sorted {} panel by {column:?}.", panel.label());
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
            self.state.status = "Rename skipped: name is unchanged.".into();
            return Ok(ViewUpdate::status());
        }

        rename_path(&source, &target)?;
        self.state
            .panel_mut(panel)
            .update_history_after_rename(&old_name, new_name.trim());
        let entries = self
            .archive_service
            .entries_for_location(&self.state.panel(panel).location)?;
        self.state.panel_mut(panel).replace_entries(entries);
        self.state.status = format!("Renamed: {}", target.display());

        Ok(ViewUpdate::panel_entries(panel))
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

        let entries = self
            .archive_service
            .entries_for_location(&self.state.panel(panel).location)?;
        self.state.panel_mut(panel).replace_entries(entries);
        self.state.status = format!("Created directory: {}", target.display());

        Ok(ViewUpdate::panel_entries(panel))
    }

    pub fn operation_request(&self, kind: FileOperationKind) -> Result<FileOperationRequest> {
        let source_panel = self.state.active_panel();
        let target_panel = self.state.inactive_panel();

        if source_panel.location.filesystem_path().is_none() {
            bail!("Archive extraction through F5 is prepared in the service layer but not yet wired into the command bar");
        }

        let sources = source_panel
            .selected_items()
            .into_iter()
            .map(|item| item.path)
            .collect::<Vec<_>>();

        if sources.is_empty() {
            bail!("No entries selected for this file operation");
        }

        let target_directory = match kind {
            FileOperationKind::Delete => None,
            FileOperationKind::Copy | FileOperationKind::Move => {
                Some(target_panel.location.host_directory())
            }
        };

        Ok(FileOperationRequest {
            kind,
            sources,
            target_directory,
        })
    }

    pub fn set_status(&mut self, status: impl Into<String>) -> ViewUpdate {
        self.state.status = status.into();
        ViewUpdate::status()
    }
}
