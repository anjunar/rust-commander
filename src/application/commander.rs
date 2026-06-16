use std::{fs, path::PathBuf};

use anyhow::{Context, Result, bail};

use crate::{
    application::{ActivePanel, app_state::AppState, commands::ViewUpdate},
    domain::{
        Panel,
        operation::{FileOperationKind, FileOperationRequest},
        panel::parent_path,
        sorting::{SortColumn, SortDirection},
    },
    fs::reader::{read_entries, rename_path},
    platform,
};

pub struct Commander {
    state: AppState,
}

impl Commander {
    pub fn new(initial_path: PathBuf) -> Result<Self> {
        let left = Panel::new(initial_path.clone(), read_entries(&initial_path)?);
        let right = Panel::new(initial_path.clone(), read_entries(&initial_path)?);
        let roots = platform::available_roots();

        Ok(Self {
            state: AppState::new(left, right, roots),
        })
    }

    pub fn state(&self) -> &AppState {
        &self.state
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

        if selected.is_dir {
            let entries = read_entries(&selected.path)?;
            self.state
                .panel_mut(panel)
                .navigate_to(selected.path.clone(), entries);
            self.state.status = format!("Opened: {}", selected.path.display());
            return Ok(ViewUpdate::panel_entries(panel));
        }

        platform::open_path(&selected.path)?;
        self.state.status = format!("Opened with default app: {}", selected.path.display());
        Ok(ViewUpdate::status())
    }

    pub fn go_parent(&mut self, panel: ActivePanel) -> Result<ViewUpdate> {
        self.state.active_panel = panel;
        let next_path = parent_path(&self.state.panel(panel).path);
        let entries = read_entries(&next_path)?;
        self.state
            .panel_mut(panel)
            .navigate_to(next_path.clone(), entries);
        self.state.status = format!("Up one level: {}", next_path.display());
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
            .navigate_to(root.path.clone(), entries);
        self.state.status = format!(
            "Switched {} panel to {}",
            panel.label(),
            root.path.display()
        );
        Ok(ViewUpdate::panel_entries(panel))
    }

    pub fn refresh_visible_panels(&mut self) -> Result<ViewUpdate> {
        self.state.roots = platform::available_roots();
        let left_entries = read_entries(&self.state.left.path)?;
        let right_entries = read_entries(&self.state.right.path)?;
        self.state.left.replace_entries(left_entries);
        self.state.right.replace_entries(right_entries);
        self.state.status = "File changes detected. View refreshed.".into();
        Ok(ViewUpdate {
            roots: true,
            ..ViewUpdate::both_panels()
        })
    }

    pub fn refresh_after_operation(&mut self, status: String) -> ViewUpdate {
        self.refresh_with_status(status)
    }

    pub fn refresh_with_status(&mut self, status: String) -> ViewUpdate {
        self.state.roots = platform::available_roots();

        match read_entries(&self.state.left.path) {
            Ok(entries) => self.state.left.replace_entries(entries),
            Err(error) => self.state.status = format!("Left refresh failed: {error}"),
        }

        match read_entries(&self.state.right.path) {
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
        let entries = read_entries(&self.state.panel(panel).path)?;
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

        let target = self.state.panel(panel).path.join(trimmed);
        if target.exists() {
            bail!("An entry with this name already exists");
        }

        fs::create_dir(&target)
            .with_context(|| format!("Could not create directory {}", target.display()))?;

        let entries = read_entries(&self.state.panel(panel).path)?;
        self.state.panel_mut(panel).replace_entries(entries);
        self.state.status = format!("Created directory: {}", target.display());

        Ok(ViewUpdate::panel_entries(panel))
    }

    pub fn operation_request(&self, kind: FileOperationKind) -> Result<FileOperationRequest> {
        let source_panel = self.state.active_panel();
        let target_panel = self.state.inactive_panel();

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
            FileOperationKind::Copy | FileOperationKind::Move => Some(target_panel.path.clone()),
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
