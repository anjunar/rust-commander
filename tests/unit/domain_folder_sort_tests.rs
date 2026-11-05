use std::path::Path;

use super::{
    lookup_folder_sort, normalize_folder_path, remove_folder_sort,
    resolve_folder_sort_for_navigation, upsert_folder_sort,
};
use crate::{
    config::FolderSortPreference,
    domain::sorting::{SortColumn, SortDirection, SortState},
};

#[test]
fn normalizes_dot_and_parent_components() {
    let path = Path::new("/tmp/foo/./bar/../baz");
    let normalized = normalize_folder_path(path);
    let expected = normalize_folder_path(Path::new("/tmp/foo/baz"));
    assert_eq!(normalized, expected);
}

#[test]
fn lookup_matches_equivalent_paths() {
    let prefs = vec![FolderSortPreference {
        path: Path::new("/tmp/photos").to_path_buf(),
        sort_column: SortColumn::Modified,
        sort_direction: SortDirection::Descending,
    }];
    let found = lookup_folder_sort(&prefs, Path::new("/tmp/photos/./"));
    assert_eq!(
        found,
        Some(SortState {
            column: SortColumn::Modified,
            direction: SortDirection::Descending,
        })
    );
}

#[test]
fn upsert_updates_existing_preference() {
    let mut prefs = Vec::new();
    upsert_folder_sort(
        &mut prefs,
        Path::new("/tmp/dl").to_path_buf(),
        SortState {
            column: SortColumn::Name,
            direction: SortDirection::Ascending,
        },
    );
    upsert_folder_sort(
        &mut prefs,
        Path::new("/tmp/dl").to_path_buf(),
        SortState {
            column: SortColumn::Size,
            direction: SortDirection::Descending,
        },
    );
    assert_eq!(prefs.len(), 1);
    assert_eq!(prefs[0].sort_column, SortColumn::Size);
    assert_eq!(prefs[0].sort_direction, SortDirection::Descending);
}

#[test]
fn remove_drops_matching_preference() {
    let mut prefs = vec![FolderSortPreference {
        path: Path::new("/tmp/dl").to_path_buf(),
        sort_column: SortColumn::Size,
        sort_direction: SortDirection::Descending,
    }];
    assert!(remove_folder_sort(&mut prefs, Path::new("/tmp/dl")));
    assert!(prefs.is_empty());
    assert!(!remove_folder_sort(&mut prefs, Path::new("/tmp/dl")));
}

#[test]
fn navigation_prefers_session_override_over_remembered_sort() {
    let prefs = vec![FolderSortPreference {
        path: Path::new("/tmp/dl").to_path_buf(),
        sort_column: SortColumn::Modified,
        sort_direction: SortDirection::Descending,
    }];
    let override_sort = SortState {
        column: SortColumn::Type,
        direction: SortDirection::Ascending,
    };
    assert_eq!(
        resolve_folder_sort_for_navigation(&prefs, Path::new("/tmp/dl"), Some(override_sort)),
        Some(override_sort)
    );
    assert_eq!(
        resolve_folder_sort_for_navigation(&prefs, Path::new("/tmp/dl"), None),
        Some(SortState {
            column: SortColumn::Modified,
            direction: SortDirection::Descending,
        })
    );
    assert_eq!(
        resolve_folder_sort_for_navigation(&[], Path::new("/tmp/dl"), Some(override_sort)),
        None
    );
}
