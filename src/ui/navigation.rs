use std::{sync::mpsc, thread};

use rust_i18n::t;

use crate::{
    application::{ActivePanel, Commander},
    archive::{ArchiveError, ArchiveService},
    domain::{Entry, PanelLocation},
    presentation,
};

pub enum SelectedNavigation {
    Load(NavigationRequest),
    Activate,
}

pub struct NavigationRequest {
    pub panel: ActivePanel,
    pub next_location: PanelLocation,
    pub status: String,
    pub busy_message: String,
}

pub struct DirectoryLoadResult {
    pub panel: ActivePanel,
    pub next_location: PanelLocation,
    pub entries: Vec<Entry>,
    pub status: String,
}

pub fn selected_navigation_request(
    commander: &Commander,
    panel: ActivePanel,
) -> SelectedNavigation {
    let Some(selected) = commander.state().panel(panel).selected_item() else {
        return SelectedNavigation::Activate;
    };

    if selected.is_parent_link {
        if let Some(next_location) = commander.state().panel(panel).location.parent() {
            return SelectedNavigation::Load(NavigationRequest {
                panel,
                status: t!("status.up_one_level", path = next_location.display_label())
                    .into_owned(),
                next_location,
                busy_message: t!("status.loading_parent_directory").into_owned(),
            });
        }

        return SelectedNavigation::Activate;
    }

    if selected.is_dir {
        let next_location = match commander.state().panel(panel).location.clone() {
            PanelLocation::Filesystem(_) => PanelLocation::filesystem(selected.path.clone()),
            PanelLocation::Archive(view) => {
                let Some(archive_path) = selected.archive_path else {
                    return SelectedNavigation::Activate;
                };
                PanelLocation::archive(view.session, archive_path)
            }
        };

        return SelectedNavigation::Load(NavigationRequest {
            panel,
            status: t!("status.opened", path = selected.path.display().to_string()).into_owned(),
            next_location,
            busy_message: t!("status.loading_directory").into_owned(),
        });
    }

    if selected.archive_path.is_none() && commander.archive_service().is_archive_path(&selected.path)
    {
        return SelectedNavigation::Load(NavigationRequest {
            panel,
            status: t!("status.opened_archive", path = selected.path.display().to_string())
                .into_owned(),
            next_location: PanelLocation::filesystem(selected.path),
            busy_message: t!("status.opening_archive").into_owned(),
        });
    }

    SelectedNavigation::Activate
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

pub fn spawn_directory_load(
    request: NavigationRequest,
    archive_service: ArchiveService,
    show_hidden_files: bool,
) -> mpsc::Receiver<Result<DirectoryLoadResult, String>> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let worker_location = request.next_location.clone();
        let actual_location = match &worker_location {
            PanelLocation::Filesystem(path) if archive_service.is_archive_path(path) && path.is_file() => {
                archive_service.archive_location_for_path(path)
            }
            _ => Ok(worker_location),
        };

        let result = actual_location
            .and_then(|location| {
                load_entries_for_location(&archive_service, &location, show_hidden_files).map(
                    |entries| DirectoryLoadResult {
                        panel: request.panel,
                        next_location: location,
                        entries,
                        status: request.status,
                    },
                )
            })
            .map_err(|error| error.to_string());
        let _ = tx.send(result);
    });

    rx
}

fn load_entries_for_location(
    archive_service: &ArchiveService,
    location: &PanelLocation,
    show_hidden_files: bool,
) -> Result<Vec<Entry>, ArchiveError> {
    match location {
        PanelLocation::Filesystem(path) => crate::fs::reader::read_entries(path, show_hidden_files)
            .map_err(|error| ArchiveError::IoError {
                detail: error.to_string(),
            }),
        PanelLocation::Archive(_) => archive_service.entries_for_location(location),
    }
}
