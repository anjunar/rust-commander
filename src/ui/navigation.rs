use std::{sync::mpsc, thread};

use rust_i18n::t;

use crate::{
    application::{ActivePanel, Commander, EntryLoader},
    archive::ArchiveService,
    domain::{Entry, PanelLocation},
    presentation,
};

pub enum SelectedNavigation {
    Load(NavigationRequest),
    OpenPath {
        path: std::path::PathBuf,
        status: String,
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
    pub generation: u64,
    pub action: LoadAction,
    pub next_location: PanelLocation,
    pub status: String,
    pub busy_message: String,
}

#[derive(Clone, Debug)]
pub struct DirectoryLoadResult {
    pub panel: ActivePanel,
    pub generation: u64,
    pub action: LoadAction,
    pub next_location: PanelLocation,
    pub entries: Vec<Entry>,
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
            return SelectedNavigation::Load(NavigationRequest {
                panel,
                generation: 0,
                action: LoadAction::Navigate,
                status: t!("status.up_one_level", path = next_location.display_label())
                    .into_owned(),
                next_location,
                busy_message: t!("status.loading_parent_directory").into_owned(),
            });
        }

        return SelectedNavigation::Unsupported {
            message: "No parent location available".into(),
        };
    }

    if selected.is_dir {
        let next_location = match commander.state().panel(panel).location.clone() {
            PanelLocation::Filesystem(_) => PanelLocation::filesystem(selected.path.clone()),
            PanelLocation::Archive(view) => {
                let Some(archive_path) = selected.archive_path else {
                    return SelectedNavigation::Unsupported {
                        message: "Archive entry is missing its path".into(),
                    };
                };
                PanelLocation::archive(view.session, archive_path)
            }
        };

        return SelectedNavigation::Load(NavigationRequest {
            panel,
            generation: 0,
            action: LoadAction::Navigate,
            status: t!("status.opened", path = selected.path.display().to_string()).into_owned(),
            next_location,
            busy_message: t!("status.loading_directory").into_owned(),
        });
    }

    if selected.archive_path.is_none() && archive_service.is_archive_path(&selected.path) {
        return SelectedNavigation::Load(NavigationRequest {
            panel,
            generation: 0,
            action: LoadAction::Navigate,
            status: t!(
                "status.opened_archive",
                path = selected.path.display().to_string()
            )
            .into_owned(),
            next_location: PanelLocation::filesystem(selected.path),
            busy_message: t!("status.opening_archive").into_owned(),
        });
    }

    match commander.state().panel(panel).location {
        PanelLocation::Archive(_) => SelectedNavigation::Unsupported {
            message: t!("error.archive_view_not_wired").into_owned(),
        },
        PanelLocation::Filesystem(_) => SelectedNavigation::OpenPath {
            status: format!("Opened with default app: {}", selected.path.display()),
            path: selected.path,
        },
    }
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
        generation: 0,
        action: LoadAction::Navigate,
        status: t!(
            "status.switched_panel",
            panel = presentation::panel_label(panel),
            path = target_path.display().to_string()
        )
        .into_owned(),
        next_location: PanelLocation::filesystem(target_path),
        busy_message: t!("status.loading_drive").into_owned(),
    })
}

pub fn refresh_request(
    commander: &Commander,
    panel: ActivePanel,
    status: String,
) -> NavigationRequest {
    NavigationRequest {
        panel,
        generation: 0,
        action: LoadAction::Refresh,
        next_location: commander.state().panel(panel).location.clone(),
        busy_message: t!("status.loading_directory").into_owned(),
        status,
    }
}

pub fn spawn_directory_load(
    request: NavigationRequest,
    archive_service: ArchiveService,
    show_hidden_files: bool,
) -> mpsc::Receiver<Result<DirectoryLoadResult, String>> {
    let (tx, rx) = mpsc::channel();
    let loader = EntryLoader::new(archive_service, show_hidden_files);

    thread::spawn(move || {
        let result = loader
            .load(request.next_location.clone())
            .map(|loaded| DirectoryLoadResult {
                panel: request.panel,
                generation: request.generation,
                action: request.action,
                next_location: loaded.location,
                entries: loaded.entries,
                status: request.status,
            })
            .map_err(|error| error.to_string());
        let _ = tx.send(result);
    });

    rx
}
