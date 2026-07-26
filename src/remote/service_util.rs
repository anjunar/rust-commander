use std::{
    env,
    io::ErrorKind,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use ssh2::FileStat;

use crate::remote::RemotePath;

use super::RemoteListingCacheKey;

pub(super) fn remote_attributes(stat: &FileStat, is_dir: bool) -> String {
    match stat.perm {
        Some(perm) => format!("{perm:o}"),
        None if is_dir => "DIR".into(),
        None => String::new(),
    }
}

pub(super) fn stat_is_directory(stat: &FileStat) -> bool {
    const S_IFMT: u32 = 0o170000;
    const S_IFDIR: u32 = 0o040000;
    stat.perm
        .map(|perm| perm & S_IFMT == S_IFDIR)
        .unwrap_or(false)
}

pub(super) fn stat_is_symlink(stat: &FileStat) -> bool {
    const S_IFMT: u32 = 0o170000;
    const S_IFLNK: u32 = 0o120000;
    stat.perm
        .map(|perm| perm & S_IFMT == S_IFLNK)
        .unwrap_or(false)
}

pub(super) fn listing_cache_key(
    session_key: &str,
    current_path: &str,
    show_hidden_files: bool,
) -> RemoteListingCacheKey {
    RemoteListingCacheKey {
        session_key: session_key.to_string(),
        path: RemotePath::new(current_path).as_str().to_string(),
        show_hidden_files,
    }
}

pub(super) fn to_remote_fs_path(path: &RemotePath) -> PathBuf {
    PathBuf::from(path.as_str())
}

pub(super) fn system_time_from_unix(value: u64) -> SystemTime {
    UNIX_EPOCH + std::time::Duration::from_secs(value)
}

pub(super) fn split_name_and_extension(name: &str) -> (String, String) {
    match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => (stem.into(), format!(".{extension}")),
        _ => (name.into(), String::new()),
    }
}

pub(super) fn describe_connect_error(address: &str, error: &std::io::Error) -> String {
    match error.kind() {
        ErrorKind::TimedOut => {
            format!("Could not reach remote host {address}: connection timed out")
        }
        ErrorKind::ConnectionRefused => {
            format!("Could not reach remote host {address}: connection was refused")
        }
        ErrorKind::NotFound | ErrorKind::AddrNotAvailable => {
            format!("Could not reach remote host {address}: host not found")
        }
        _ => format!("Could not connect to remote host {address}: {error}"),
    }
}

pub(super) fn describe_ssh_session_error(action: &str, error: &ssh2::Error) -> String {
    let detail = error.message();
    if contains_any(detail, &["timed out", "socket timeout"]) {
        format!("SSH session timed out while trying to {action}: {detail}")
    } else {
        format!("Could not {action}: {detail}")
    }
}

pub(super) fn describe_auth_error(
    method: &str,
    username: &str,
    profile: &crate::remote::RemoteProfile,
    error: &ssh2::Error,
) -> String {
    let detail = error.message();
    if contains_any(
        detail,
        &[
            "authentication failed",
            "username/publickey combination invalid",
        ],
    ) {
        format!(
            "Authentication failed for {}@{}:{} using {} authentication: {}",
            username, profile.host, profile.port, method, detail
        )
    } else if contains_any(detail, &["timed out", "socket timeout"]) {
        format!(
            "Authentication timed out for {}@{}:{} using {} authentication: {}",
            username, profile.host, profile.port, method, detail
        )
    } else {
        format!(
            "Could not authenticate {}@{}:{} using {} authentication: {}",
            username, profile.host, profile.port, method, detail
        )
    }
}

pub(super) fn describe_sftp_path_error(action: &str, path: &str, error: &ssh2::Error) -> String {
    let detail = error.message();
    if is_remote_not_found_error(error) {
        format!("Remote path not found while trying to {action}: {path} ({detail})")
    } else if contains_any(detail, &["permission denied", "not authorized"]) {
        format!("Permission denied while trying to {action}: {path} ({detail})")
    } else if contains_any(detail, &["file already exists"]) {
        format!("Remote target already exists: {path} ({detail})")
    } else if contains_any(detail, &["not a directory"]) {
        format!("Remote path is not a directory: {path} ({detail})")
    } else if contains_any(detail, &["timed out", "socket timeout"]) {
        format!("SFTP operation timed out while trying to {action}: {path} ({detail})")
    } else {
        format!("Could not {action} {path}: {detail}")
    }
}

pub(super) fn is_remote_not_found_error(error: &ssh2::Error) -> bool {
    contains_any(error.message(), &["no such file", "no such path"])
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    let value = value.to_ascii_lowercase();
    needles.iter().any(|needle| value.contains(needle))
}

pub(super) fn find_known_hosts_path() -> Option<PathBuf> {
    let home = user_home_dir()?;
    known_hosts_candidates(&home)
        .into_iter()
        .find(|path| path.is_file())
}

pub(super) fn known_hosts_write_path() -> Option<PathBuf> {
    let home = user_home_dir()?;
    known_hosts_candidates(&home).into_iter().next()
}

pub(super) fn known_hosts_entry_name(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_string()
    } else {
        format!("[{host}]:{port}")
    }
}

pub(super) fn user_home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .or_else(|| env::var_os("USERPROFILE").filter(|value| !value.is_empty()))
        .or_else(|| {
            let drive = env::var_os("HOMEDRIVE")?;
            let path = env::var_os("HOMEPATH")?;
            if drive.is_empty() || path.is_empty() {
                None
            } else {
                let mut combined = PathBuf::from(drive);
                combined.push(path);
                Some(combined.into_os_string())
            }
        })
        .map(PathBuf::from)
}

pub(super) fn known_hosts_candidates(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".ssh").join("known_hosts"),
        home.join(".ssh").join("known_hosts2"),
    ]
}

pub(super) fn format_fingerprint(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}
