use std::path::{Component, Path, PathBuf};

use crate::{config::FolderSortPreference, domain::sorting::SortState};

/// Normalize a filesystem directory path for folder-sort lookup.
///
/// - resolves `.` / `..`
/// - strips a trailing separator (except drive roots)
/// - on Windows, compares case-insensitively via lowercased key
pub fn normalize_folder_path(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };

    let mut out = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::Prefix(prefix) => out.push(prefix.as_os_str()),
            Component::RootDir => out.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(part) => out.push(part),
        }
    }

    #[cfg(windows)]
    {
        PathBuf::from(out.to_string_lossy().to_lowercase())
    }
    #[cfg(not(windows))]
    {
        out
    }
}

pub fn lookup_folder_sort(prefs: &[FolderSortPreference], path: &Path) -> Option<SortState> {
    let key = normalize_folder_path(path);
    prefs
        .iter()
        .find(|pref| normalize_folder_path(&pref.path) == key)
        .map(FolderSortPreference::sort_state)
}

/// Resolve sort for navigating into `path`.
///
/// When a remembered folder preference exists, a matching session override wins
/// for the rest of the process lifetime; otherwise the preference is applied.
/// Returns `None` when there is no remembered preference (caller keeps current sort).
pub fn resolve_folder_sort_for_navigation(
    prefs: &[FolderSortPreference],
    path: &Path,
    session_override: Option<SortState>,
) -> Option<SortState> {
    let preferred = lookup_folder_sort(prefs, path)?;
    Some(session_override.unwrap_or(preferred))
}

pub fn upsert_folder_sort(prefs: &mut Vec<FolderSortPreference>, path: PathBuf, sort: SortState) {
    let key = normalize_folder_path(&path);
    if let Some(existing) = prefs
        .iter_mut()
        .find(|pref| normalize_folder_path(&pref.path) == key)
    {
        existing.path = path;
        existing.sort_column = sort.column;
        existing.sort_direction = sort.direction;
        return;
    }
    prefs.push(FolderSortPreference {
        path,
        sort_column: sort.column,
        sort_direction: sort.direction,
    });
    prefs.sort_by(|left, right| left.path.cmp(&right.path));
}

pub fn remove_folder_sort(prefs: &mut Vec<FolderSortPreference>, path: &Path) -> bool {
    let key = normalize_folder_path(path);
    let before = prefs.len();
    prefs.retain(|pref| normalize_folder_path(&pref.path) != key);
    prefs.len() != before
}

#[cfg(test)]
#[path = "../../tests/unit/domain_folder_sort_tests.rs"]
mod tests;
