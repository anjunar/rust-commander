use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    sync::mpsc,
};

use rust_i18n::t;

use crate::{
    application::{
        ActivePanel, Commander, EntryLoader, NavigationError, SessionStore, TaskSpawner,
    },
    archive::ArchiveService,
    domain::{
        sorting::{sort_entries, SortState},
        Entry, PanelLocation, SelectionIntent, TabId,
    },
    presentation,
    remote::RemoteService,
};

pub enum SelectedNavigation {
    Load(Box<NavigationRequest>),
    OpenPath {
        path: std::path::PathBuf,
        status: String,
    },
    OpenArchive {
        path: std::path::PathBuf,
    },
    Unsupported {
        message: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoadAction {
    Navigate,
    Refresh,
}

#[derive(Clone, Debug)]
pub struct NavigationRequest {
    pub panel: ActivePanel,
    /// Active tab when the request was created; stale loads for other tabs are ignored.
    pub tab_id: TabId,
    pub generation: u64,
    pub action: LoadAction,
    pub next_location: PanelLocation,
    pub selection_intent: Option<SelectionIntent>,
    pub status: String,
    pub busy_message: String,
    /// Filled by NavigationController before spawn; used to sort on the worker.
    pub sort_state: SortState,
    pub folders_first: bool,
}

#[derive(Clone, Debug)]
pub struct DirectoryLoadResult {
    pub panel: ActivePanel,
    pub tab_id: TabId,
    pub generation: u64,
    pub action: LoadAction,
    pub next_location: PanelLocation,
    pub entries: Vec<Entry>,
    pub selection_intent: Option<SelectionIntent>,
    pub status: String,
}

pub fn selected_navigation_request(
    commander: &Commander,
    archive_service: &ArchiveService,
    panel: ActivePanel,
) -> SelectedNavigation {
    let Some(selected) = commander.state().panel(panel).selected_item() else {
        return SelectedNavigation::Unsupported {
            message: "No entry selected".into(),
        };
    };

    if selected.is_parent_link {
        if let Some(next_location) = commander.state().panel(panel).location.parent() {
            return SelectedNavigation::Load(Box::new(NavigationRequest {
                panel,
                tab_id: 0,
                generation: 0,
                action: LoadAction::Navigate,
                status: t!("status.up_one_level", path = next_location.display_label())
                    .into_owned(),
                next_location,
                selection_intent: None,
                busy_message: t!("status.loading_parent_directory").into_owned(),
                sort_state: SortState::default(),
                folders_first: true,
            }));
        }

        return SelectedNavigation::Unsupported {
            message: "No parent location available".into(),
        };
    }

    // Archives before directories: on Windows, `.7z` must never go through
    // `read_dir` / shell-folder navigation (7-Zip shell extensions can abort).
    if let Some(path) = selected.filesystem_path.clone() {
        if selected.archive_path.is_none() && archive_service.is_archive_path(&path) {
            return SelectedNavigation::OpenArchive { path };
        }
    }

    if selected.is_dir {
        let next_location = match commander.state().panel(panel).location.clone() {
            PanelLocation::Filesystem(_) => {
                let Some(path) = selected.filesystem_path.clone() else {
                    return SelectedNavigation::Unsupported {
                        message: "Filesystem entry is missing its path".into(),
                    };
                };
                PanelLocation::filesystem(crate::fs::reader::resolve_directory_navigation_path(
                    &path,
                ))
            }
            PanelLocation::Archive(view) => {
                let Some(archive_path) = selected.archive_path else {
                    return SelectedNavigation::Unsupported {
                        message: "Archive entry is missing its path".into(),
                    };
                };
                PanelLocation::archive(
                    view.session_key.clone(),
                    view.archive_path.clone(),
                    archive_path,
                )
            }
            PanelLocation::Remote(location) => {
                let Some(remote_path) = selected.remote_path else {
                    return SelectedNavigation::Unsupported {
                        message: "Remote entry is missing its path".into(),
                    };
                };
                PanelLocation::remote(
                    location.session_key.clone(),
                    location.username.clone(),
                    location.host.clone(),
                    location.port,
                    remote_path,
                )
            }
        };

        return SelectedNavigation::Load(Box::new(NavigationRequest {
            panel,
                tab_id: 0,
            generation: 0,
            action: LoadAction::Navigate,
            status: t!("status.opened", path = selected.display_path.as_str()).into_owned(),
            next_location,
            selection_intent: None,
            busy_message: t!("status.loading_directory").into_owned(),
            sort_state: SortState::default(),
            folders_first: true,
        }));
    }

    match commander.state().panel(panel).location {
        PanelLocation::Archive(_) | PanelLocation::Remote(_) => SelectedNavigation::Unsupported {
            message: t!("error.archive_view_not_wired").into_owned(),
        },
        PanelLocation::Filesystem(_) => match selected.filesystem_path {
            Some(path) => SelectedNavigation::OpenPath {
                status: format!("Opened with default app: {}", selected.display_path),
                path,
            },
            None => SelectedNavigation::Unsupported {
                message: "Filesystem entry is missing its path".into(),
            },
        },
    }
}

pub fn parent_navigation_request(
    commander: &Commander,
    panel: ActivePanel,
) -> Option<NavigationRequest> {
    let next_location = commander.state().panel(panel).location.parent()?;
    Some(NavigationRequest {
        panel,
                tab_id: 0,
        generation: 0,
        action: LoadAction::Navigate,
        status: t!("status.up_one_level", path = next_location.display_label()).into_owned(),
        next_location,
        selection_intent: None,
        busy_message: t!("status.loading_parent_directory").into_owned(),
        sort_state: SortState::default(),
        folders_first: true,
    })
}

pub fn root_navigation_request(
    commander: &Commander,
    panel: ActivePanel,
    index: usize,
) -> Option<NavigationRequest> {
    let target_path = commander
        .state()
        .roots
        .get(index)
        .map(|root| root.path.clone())?;

    Some(NavigationRequest {
        panel,
                tab_id: 0,
        generation: 0,
        action: LoadAction::Navigate,
        status: t!(
            "status.switched_panel",
            panel = presentation::panel_label(panel),
            path = target_path.display().to_string()
        )
        .into_owned(),
        next_location: PanelLocation::filesystem(target_path),
        selection_intent: None,
        busy_message: t!("status.loading_drive").into_owned(),
        sort_state: SortState::default(),
        folders_first: true,
    })
}

pub fn filesystem_navigation_request(
    panel: ActivePanel,
    target_path: PathBuf,
) -> NavigationRequest {
    NavigationRequest {
        panel,
                tab_id: 0,
        generation: 0,
        action: LoadAction::Navigate,
        status: t!(
            "status.opened_favorite",
            path = target_path.display().to_string()
        )
        .into_owned(),
        next_location: PanelLocation::filesystem(target_path),
        selection_intent: None,
        busy_message: t!("status.loading_directory").into_owned(),
        sort_state: SortState::default(),
        folders_first: true,
    }
}

pub fn location_root_navigation_request(
    commander: &Commander,
    panel: ActivePanel,
) -> NavigationRequest {
    let next_location = commander.state().panel(panel).location.root();
    NavigationRequest {
        panel,
                tab_id: 0,
        generation: 0,
        action: LoadAction::Navigate,
        status: t!(
            "status.opened_location_root",
            path = next_location.display_label()
        )
        .into_owned(),
        next_location,
        selection_intent: Some(SelectionIntent::select_first()),
        busy_message: t!("status.loading_directory").into_owned(),
        sort_state: SortState::default(),
        folders_first: true,
    }
}

pub fn refresh_request(
    commander: &Commander,
    panel: ActivePanel,
    status: String,
) -> NavigationRequest {
    NavigationRequest {
        panel,
                tab_id: 0,
        generation: 0,
        action: LoadAction::Refresh,
        next_location: commander.state().panel(panel).location.clone(),
        selection_intent: Some(commander.state().panel(panel).refresh_selection_intent()),
        busy_message: t!("status.loading_directory").into_owned(),
        status,
        sort_state: SortState::default(),
        folders_first: true,
    }
}

pub fn spawn_directory_load(
    task_spawner: TaskSpawner,
    request: NavigationRequest,
    archive_service: ArchiveService,
    remote_service: RemoteService,
    session_store: Rc<RefCell<SessionStore>>,
    show_hidden_files: bool,
) -> mpsc::Receiver<Result<DirectoryLoadResult, Box<NavigationError>>> {
    let (tx, rx) = mpsc::channel();
    let session_store = session_store.borrow().clone();
    let force_refresh = matches!(request.action, LoadAction::Refresh);
    let loader = EntryLoader::new(
        archive_service,
        remote_service,
        session_store,
        show_hidden_files,
        force_refresh,
    );

    task_spawner.spawn(move || {
        let result = loader
            .load(request.next_location.clone())
            .map(|loaded| {
                let mut entries = loaded.entries;
                sort_entries(&mut entries, request.sort_state, request.folders_first);
                DirectoryLoadResult {
                    panel: request.panel,
                    tab_id: request.tab_id,
                    generation: request.generation,
                    action: request.action,
                    next_location: loaded.location,
                    entries,
                    selection_intent: request.selection_intent,
                    status: request.status,
                }
            });
        let _ = tx.send(result);
    });

    rx
}
