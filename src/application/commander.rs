use std::path::PathBuf;

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
        sorting::{SortColumn, SortDirection},
        Entry, Panel, PanelLocation, RootLocation,
    },
};

pub struct Commander {
    state: AppState,
    panel_settings: PanelSettings,
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
        self.state
            .panel_mut(panel)
            .set_sort_state(column, direction);
        self.state.status = status;
        ViewUpdate::panel_entries(panel)
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

    pub fn create_directory_request(&self, name: &str) -> Result<PathBuf> {
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

        Ok(target)
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

    pub fn queue_selection_after_file_operation(&mut self, plan: &OperationPlan) {
        if !matches!(plan.kind(), FileOperationKind::Delete | FileOperationKind::Move) {
            return;
        }

        let Some(source_directory) = plan
            .local_sources()
            .first()
            .and_then(|source| source.parent().map(std::path::Path::to_path_buf))
        else {
            return;
        };

        for panel in [ActivePanel::Left, ActivePanel::Right] {
            let matches_source_directory = self
                .state
                .panel(panel)
                .location
                .filesystem_path()
                .map(|path| path == source_directory.as_path())
                .unwrap_or(false);
            if matches_source_directory {
                self.state.panel_mut(panel).queue_delete_selection();
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, time::SystemTime};

    use crate::{
        application::{FileOperationKind, OperationPlan},
        config::PanelSettings,
        domain::{Entry, EntryKind, PanelLocation},
        remote::{RemoteAuthConfig, RemotePath, RemoteProfile, RemoteRuntimeSecret, RemoteSession},
    };

    use super::{ActivePanel, Commander, SessionStore};

    #[test]
    fn delete_request_allows_remote_in_inactive_panel() {
        let mut commander = Commander::new(
            PathBuf::from("/tmp/left"),
            PathBuf::from("/tmp/right"),
            PanelSettings::default(),
            Vec::new(),
            "Ready".into(),
        )
        ;

        commander.state.active_panel = ActivePanel::Left;
        commander.state.left.location = PanelLocation::filesystem(PathBuf::from("/tmp/left"));
        commander.state.left.entries = vec![file_entry("keep.txt"), file_entry("delete.txt")];
        commander.state.left.select_single(1);
        let mut session_store = SessionStore::default();
        let session_key = session_store.insert_remote(remote_session());
        commander.state.right.location = PanelLocation::remote(
            session_key,
            "tester",
            "example.com",
            22,
            "/home/test",
        );

        let request = commander
            .operation_request(FileOperationKind::Delete, &session_store)
            .unwrap();

        match request {
            OperationPlan::Local(local) => {
                assert_eq!(local.sources, vec![PathBuf::from("/tmp/left/delete.txt")]);
                assert!(local.target_directory.is_none());
            }
            _ => panic!("expected local operation plan"),
        }
    }

    fn file_entry(name: &str) -> Entry {
        Entry {
            name: name.into(),
            archive_path: None,
            remote_path: None,
            kind: EntryKind::File,
            is_dir: false,
            size_bytes: 1,
            modified_at: Some(SystemTime::now()),
            attributes: String::new(),
            is_parent_link: false,
        }
    }

    fn remote_session() -> RemoteSession {
        RemoteSession::new(
            RemoteProfile {
                name: "test".into(),
                host: "example.com".into(),
                port: 22,
                auth: RemoteAuthConfig::Password {
                    username: "tester".into(),
                },
                start_directory: RemotePath::new("/home/test"),
                skip_host_key_verification: false,
            },
            RemoteRuntimeSecret::Password("secret".into()),
        )
    }
}
