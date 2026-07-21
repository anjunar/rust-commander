use std::path::{Component, Path, PathBuf};

use crate::domain::{entry::Entry, entry_key::EntryKey};

#[derive(Clone, Debug)]
pub enum PanelLocation {
    Filesystem(PathBuf),
    Archive(ArchiveView),
    Remote(RemoteView),
}

#[derive(Clone, Debug)]
pub struct ArchiveView {
    pub session_key: String,
    pub archive_path: PathBuf,
    pub current_path: String,
}

#[derive(Clone, Debug)]
pub struct RemoteView {
    pub session_key: String,
    pub username: String,
    pub host: String,
    pub port: u16,
    pub current_path: String,
}

impl PanelLocation {
    pub fn filesystem(path: PathBuf) -> Self {
        Self::Filesystem(path)
    }

    pub fn archive(
        session_key: impl Into<String>,
        archive_path: PathBuf,
        current_path: impl Into<String>,
    ) -> Self {
        Self::Archive(ArchiveView {
            session_key: session_key.into(),
            archive_path,
            current_path: current_path.into(),
        })
    }

    pub fn remote(
        session_key: impl Into<String>,
        username: impl Into<String>,
        host: impl Into<String>,
        port: u16,
        current_path: impl Into<String>,
    ) -> Self {
        Self::Remote(RemoteView {
            session_key: session_key.into(),
            username: username.into(),
            host: host.into(),
            port,
            current_path: normalize_remote_path(current_path.into()),
        })
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
                view.archive_path
                    .parent()
                    .unwrap_or(view.archive_path.as_path())
                    .to_path_buf(),
            ),
            Self::Remote(_) => None,
        }
    }

    pub fn display_label(&self) -> String {
        match self {
            Self::Filesystem(path) => path.display().to_string(),
            Self::Archive(view) => {
                let archive = view.archive_path.display();
                if view.current_path.is_empty() {
                    format!("{archive}!")
                } else {
                    format!("{archive}!/{}", view.current_path)
                }
            }
            Self::Remote(location) => format!(
                "{}@{}:{}",
                location.username, location.host, location.current_path
            ),
        }
    }

    /// Short label for a tab title.
    pub fn tab_title(&self) -> String {
        match self {
            Self::Filesystem(path) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .filter(|n| !n.is_empty());
                name.unwrap_or_else(|| path.display().to_string())
            }
            Self::Archive(view) => {
                let archive_name = view
                    .archive_path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| view.archive_path.display().to_string());
                if view.current_path.is_empty() {
                    archive_name
                } else {
                    view.current_path
                        .rsplit('/')
                        .find(|part| !part.is_empty())
                        .unwrap_or(archive_name.as_str())
                        .to_string()
                }
            }
            Self::Remote(location) => {
                let leaf = location
                    .current_path
                    .rsplit('/')
                    .find(|part| !part.is_empty())
                    .unwrap_or("/");
                format!("{}@{}", location.host, leaf)
            }
        }
    }

    pub fn history_key(&self) -> String {
        match self {
            Self::Filesystem(path) => format!("fs:{}", path.display()),
            Self::Archive(view) => format!(
                "archive:{}!{}",
                view.archive_path.display(),
                view.current_path
            ),
            Self::Remote(location) => format!(
                "remote:{}@{}:{}:{}",
                location.username, location.host, location.port, location.current_path
            ),
        }
    }

    /// Volume/drive root for filesystem, archive root, or remote `/`.
    pub fn root(&self) -> Self {
        match self {
            Self::Filesystem(path) => Self::Filesystem(filesystem_volume_root(path)),
            Self::Archive(view) => Self::archive(
                view.session_key.clone(),
                view.archive_path.clone(),
                String::new(),
            ),
            Self::Remote(location) => Self::remote(
                location.session_key.clone(),
                location.username.clone(),
                location.host.clone(),
                location.port,
                "/",
            ),
        }
    }

    pub fn parent(&self) -> Option<Self> {
        match self {
            Self::Filesystem(path) => {
                let parent = path.parent()?;
                Some(Self::Filesystem(parent.to_path_buf()))
            }
            Self::Archive(view) => {
                if view.current_path.is_empty() {
                    return Some(Self::Filesystem(
                        view.archive_path
                            .parent()
                            .unwrap_or(view.archive_path.as_path())
                            .to_path_buf(),
                    ));
                }

                let parent = view
                    .current_path
                    .rsplit_once('/')
                    .map(|(parent, _)| parent.to_string())
                    .unwrap_or_default();
                Some(Self::archive(
                    view.session_key.clone(),
                    view.archive_path.clone(),
                    parent,
                ))
            }
            Self::Remote(location) => remote_parent_path(&location.current_path).map(|parent| {
                Self::remote(
                    location.session_key.clone(),
                    location.username.clone(),
                    location.host.clone(),
                    location.port,
                    parent,
                )
            }),
        }
    }

    /// Entry to focus in `next_location` when leaving this location by going up one level.
    pub fn entry_key_when_leaving_to(&self, next_location: &Self) -> Option<EntryKey> {
        let parent = self.parent()?;
        if parent.history_key() != next_location.history_key() {
            return None;
        }

        match self {
            Self::Filesystem(path) => path
                .file_name()
                .map(|name| EntryKey::FilesystemName(name.to_os_string())),
            Self::Archive(view) if view.current_path.is_empty() => view
                .archive_path
                .file_name()
                .map(|name| EntryKey::FilesystemName(name.to_os_string())),
            Self::Archive(view) => {
                let child = view
                    .current_path
                    .rsplit_once('/')
                    .map(|(_, child)| child)
                    .unwrap_or(view.current_path.as_str());
                if child.is_empty() {
                    None
                } else {
                    Some(EntryKey::ArchiveEntry(view.current_path.clone()))
                }
            }
            Self::Remote(location) => {
                let child = location
                    .current_path
                    .rsplit_once('/')
                    .map(|(_, child)| child)
                    .unwrap_or(location.current_path.as_str());
                if child.is_empty() || location.current_path == "/" {
                    None
                } else {
                    Some(EntryKey::RemoteEntry(location.current_path.clone()))
                }
            }
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
                    format!("{}!/{}", view.archive_path.display(), archive_path)
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
                    format!("{}@{}:{}", location.username, location.host, remote_path)
                } else {
                    self.display_label()
                }
            }
        }
    }
}

fn normalize_remote_path(path: String) -> String {
    let mut parts = Vec::new();
    let replaced = path.replace('\\', "/");
    for part in replaced.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            value => parts.push(value),
        }
    }

    if parts.is_empty() {
        "/".into()
    } else {
        format!("/{}", parts.join("/"))
    }
}

fn remote_parent_path(path: &str) -> Option<String> {
    let normalized = normalize_remote_path(path.to_string());
    if normalized == "/" {
        return None;
    }

    let trimmed = normalized.trim_end_matches('/');
    match trimmed.rsplit_once('/') {
        Some(("", _)) | None => Some("/".into()),
        Some((parent, _)) => Some(parent.to_string()),
    }
}

fn filesystem_volume_root(path: &Path) -> PathBuf {
    let root: PathBuf = path
        .components()
        .take_while(|component| {
            matches!(component, Component::Prefix(_) | Component::RootDir)
        })
        .collect();
    if root.as_os_str().is_empty() {
        path.to_path_buf()
    } else {
        root
    }
}
