use std::path::{Path, PathBuf};

use crate::{archive::ArchiveSession, domain::entry::Entry};

#[derive(Clone, Debug)]
pub enum PanelLocation {
    Filesystem(PathBuf),
    Archive(ArchiveView),
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

    pub fn filesystem_path(&self) -> Option<&Path> {
        match self {
            Self::Filesystem(path) => Some(path.as_path()),
            Self::Archive(_) => None,
        }
    }

    pub fn host_directory(&self) -> PathBuf {
        match self {
            Self::Filesystem(path) => path.clone(),
            Self::Archive(view) => view
                .session
                .archive_path()
                .parent()
                .unwrap_or_else(|| view.session.archive_path())
                .to_path_buf(),
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
        }
    }

    pub fn entry_display_path(&self, entry: &Entry) -> PathBuf {
        match self {
            Self::Filesystem(path) => {
                if entry.is_parent_link {
                    path.parent().unwrap_or(path.as_path()).to_path_buf()
                } else {
                    path.join(&entry.name)
                }
            }
            Self::Archive(view) => {
                let label = if entry.is_parent_link {
                    self.parent()
                        .map(|parent| parent.display_label())
                        .unwrap_or_else(|| self.display_label())
                } else if let Some(archive_path) = &entry.archive_path {
                    format!("{}!/{}", view.session.archive_path().display(), archive_path)
                } else {
                    self.display_label()
                };
                PathBuf::from(label)
            }
        }
    }
}
