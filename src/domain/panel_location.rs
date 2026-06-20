use std::path::{Path, PathBuf};

use crate::{
    archive::ArchiveSession,
    domain::entry::Entry,
    remote::{RemoteLocation, RemotePath, RemoteSession},
};

#[derive(Clone, Debug)]
pub enum PanelLocation {
    Filesystem(PathBuf),
    Archive(ArchiveView),
    Remote(RemoteLocation),
}

#[derive(Clone, Debug)]
pub struct ArchiveView {
    pub session: ArchiveSession,
    pub current_path: String,
}

impl PanelLocation {
    pub fn filesystem(path: PathBuf) -> Self {
        Self::Filesystem(path)
    }

    pub fn archive(session: ArchiveSession, current_path: impl Into<String>) -> Self {
        Self::Archive(ArchiveView {
            session,
            current_path: current_path.into(),
        })
    }

    pub fn remote(session: RemoteSession, current_path: RemotePath) -> Self {
        Self::Remote(RemoteLocation::new(session, current_path))
    }

    pub fn filesystem_path(&self) -> Option<&Path> {
        match self {
            Self::Filesystem(path) => Some(path.as_path()),
            Self::Archive(_) | Self::Remote(_) => None,
        }
    }

    pub fn host_directory(&self) -> Option<PathBuf> {
        match self {
            Self::Filesystem(path) => Some(path.clone()),
            Self::Archive(view) => Some(
                view.session
                    .archive_path()
                    .parent()
                    .unwrap_or_else(|| view.session.archive_path())
                    .to_path_buf(),
            ),
            Self::Remote(_) => None,
        }
    }

    pub fn display_label(&self) -> String {
        match self {
            Self::Filesystem(path) => path.display().to_string(),
            Self::Archive(view) => {
                let archive = view.session.archive_path().display();
                if view.current_path.is_empty() {
                    format!("{archive}!")
                } else {
                    format!("{archive}!/{}", view.current_path)
                }
            }
            Self::Remote(location) => format!(
                "{}@{}:{}",
                location.session.profile().auth.username(),
                location.session.profile().host,
                location.current_path
            ),
        }
    }

    pub fn history_key(&self) -> String {
        match self {
            Self::Filesystem(path) => format!("fs:{}", path.display()),
            Self::Archive(view) => format!(
                "archive:{}!{}",
                view.session.archive_path().display(),
                view.current_path
            ),
            Self::Remote(location) => format!(
                "remote:{}@{}:{}:{}",
                location.session.profile().auth.username(),
                location.session.profile().host,
                location.session.profile().port,
                location.current_path
            ),
        }
    }

    pub fn parent(&self) -> Option<Self> {
        match self {
            Self::Filesystem(path) => Some(Self::Filesystem(
                path.parent().unwrap_or(path.as_path()).to_path_buf(),
            )),
            Self::Archive(view) => {
                if view.current_path.is_empty() {
                    return Some(Self::Filesystem(
                        view.session
                            .archive_path()
                            .parent()
                            .unwrap_or_else(|| view.session.archive_path())
                            .to_path_buf(),
                    ));
                }

                let parent = view
                    .current_path
                    .rsplit_once('/')
                    .map(|(parent, _)| parent.to_string())
                    .unwrap_or_default();
                Some(Self::archive(view.session.clone(), parent))
            }
            Self::Remote(location) => location
                .current_path
                .parent()
                .map(|parent| Self::remote(location.session.clone(), parent)),
        }
    }

    pub fn entry_filesystem_path(&self, entry: &Entry) -> Option<PathBuf> {
        match self {
            Self::Filesystem(path) => Some(if entry.is_parent_link {
                path.parent().unwrap_or(path.as_path()).to_path_buf()
            } else {
                path.join(&entry.name)
            }),
            Self::Archive(_) | Self::Remote(_) => None,
        }
    }

    pub fn entry_display_path(&self, entry: &Entry) -> String {
        match self {
            Self::Filesystem(path) => {
                if entry.is_parent_link {
                    path.parent()
                        .unwrap_or(path.as_path())
                        .display()
                        .to_string()
                } else {
                    path.join(&entry.name).display().to_string()
                }
            }
            Self::Archive(view) => {
                if entry.is_parent_link {
                    self.parent()
                        .map(|parent| parent.display_label())
                        .unwrap_or_else(|| self.display_label())
                } else if let Some(archive_path) = &entry.archive_path {
                    format!(
                        "{}!/{}",
                        view.session.archive_path().display(),
                        archive_path
                    )
                } else {
                    self.display_label()
                }
            }
            Self::Remote(location) => {
                if entry.is_parent_link {
                    self.parent()
                        .map(|parent| parent.display_label())
                        .unwrap_or_else(|| self.display_label())
                } else if let Some(remote_path) = &entry.remote_path {
                    format!(
                        "{}@{}:{}",
                        location.session.profile().auth.username(),
                        location.session.profile().host,
                        remote_path
                    )
                } else {
                    self.display_label()
                }
            }
        }
    }
}
