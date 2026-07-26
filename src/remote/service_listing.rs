use anyhow::{anyhow, bail, Result};
use ssh2::FileStat;

use crate::domain::{Entry, EntryKind};

use super::{
    util::{
        describe_sftp_path_error, listing_cache_key, remote_attributes, stat_is_directory,
        stat_is_symlink, system_time_from_unix, to_remote_fs_path,
    },
    RemoteService,
};
use crate::remote::{RemotePath, RemoteSession};

impl RemoteService {
    pub fn read_entries(
        &self,
        session_key: &str,
        session: &RemoteSession,
        current_path: &str,
        show_hidden_files: bool,
        force_refresh: bool,
    ) -> Result<Vec<Entry>> {
        let key = listing_cache_key(session_key, current_path, show_hidden_files);
        if !force_refresh {
            if let Ok(cache) = self.listing_cache.lock() {
                if let Some(entries) = cache.get(&key) {
                    return Ok(entries.clone());
                }
            }
        }

        let entries = self.fetch_directory_entries(session, current_path, show_hidden_files)?;
        if let Ok(mut cache) = self.listing_cache.lock() {
            cache.insert(key, entries.clone());
        }
        Ok(entries)
    }

    /// Drop cached listings for a remote directory (any session).
    pub fn invalidate_listing_cache_for_path(&self, current_path: &str) {
        let path = RemotePath::new(current_path).as_str().to_string();
        if let Ok(mut cache) = self.listing_cache.lock() {
            cache.retain(|key, _| key.path != path);
        }
    }

    fn fetch_directory_entries(
        &self,
        session: &RemoteSession,
        current_path: &str,
        show_hidden_files: bool,
    ) -> Result<Vec<Entry>> {
        let connection = self.connect_sftp(session)?;
        let current_path = RemotePath::new(current_path);
        let mut entries = Vec::new();
        if let Some(parent) = current_path.parent() {
            let mut parent_entry = Entry::parent_link();
            parent_entry.remote_path = Some(parent.to_string());
            entries.push(parent_entry);
        }

        let remote_dir = to_remote_fs_path(&current_path);
        let mut listed = connection
            .sftp
            .readdir(remote_dir.as_path())
            .map_err(|error| {
                anyhow!(describe_sftp_path_error(
                    "read remote directory",
                    current_path.as_str(),
                    &error
                ))
            })?;
        listed.sort_by(|(left, _), (right, _)| left.cmp(right));

        for (path, stat) in listed.drain(..) {
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if matches!(name, "." | "..") {
                continue;
            }
            if !show_hidden_files && name.starts_with('.') {
                continue;
            }

            let remote_path = current_path.join(name);
            let is_symlink = stat_is_symlink(&stat);
            let is_dir = if is_symlink {
                connection
                    .sftp
                    .stat(path.as_path())
                    .map(|followed| stat_is_directory(&followed))
                    .unwrap_or(false)
            } else {
                stat_is_directory(&stat)
            };
            let size_bytes = if is_dir { 0 } else { stat.size.unwrap_or(0) };
            let modified_at = stat.mtime.map(system_time_from_unix);
            entries.push(Entry {
                name: name.into(),
                archive_path: None,
                remote_path: Some(remote_path.to_string()),
                kind: if is_symlink {
                    EntryKind::Symlink
                } else if is_dir {
                    EntryKind::Directory
                } else {
                    EntryKind::File
                },
                is_dir,
                size_bytes,
                modified_at,
                attributes: remote_attributes(&stat, is_dir),
                is_parent_link: false,
            });
        }

        Ok(entries)
    }

    /// Create a single remote directory (does not create missing parents).
    pub fn create_directory(&self, session: &RemoteSession, path: &str) -> Result<()> {
        let connection = self.connect_sftp(session)?;
        let remote_path = RemotePath::new(path);
        if remote_path.is_root() {
            bail!("Cannot create the remote root directory");
        }

        let remote_fs_path = to_remote_fs_path(&remote_path);
        if connection.sftp.stat(remote_fs_path.as_path()).is_ok() {
            bail!("An entry with this name already exists");
        }

        connection
            .sftp
            .mkdir(remote_fs_path.as_path(), 0o755)
            .map_err(|error| {
                anyhow!(describe_sftp_path_error(
                    "create remote directory",
                    remote_path.as_str(),
                    &error
                ))
            })?;
        Ok(())
    }

    /// Change remote Unix permission bits (`mode` is masked to `0o7777`).
    pub fn set_permissions(
        &self,
        session: &RemoteSession,
        path: &str,
        mode: u32,
    ) -> Result<()> {
        let connection = self.connect_sftp(session)?;
        let remote_path = RemotePath::new(path);
        let remote_fs_path = to_remote_fs_path(&remote_path);

        let current = connection
            .sftp
            .stat(remote_fs_path.as_path())
            .map_err(|error| {
                anyhow!(describe_sftp_path_error(
                    "read remote permissions",
                    remote_path.as_str(),
                    &error
                ))
            })?;
        const S_IFMT: u32 = 0o170000;
        let file_type = current.perm.unwrap_or(0) & S_IFMT;
        let next_perm = file_type | (mode & 0o7777);

        connection
            .sftp
            .setstat(
                remote_fs_path.as_path(),
                FileStat {
                    size: None,
                    uid: None,
                    gid: None,
                    perm: Some(next_perm),
                    mtime: None,
                    atime: None,
                },
            )
            .map_err(|error| {
                anyhow!(describe_sftp_path_error(
                    "set remote permissions",
                    remote_path.as_str(),
                    &error
                ))
            })?;
        Ok(())
    }
}
