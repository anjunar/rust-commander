use std::{fs, path::Path, sync::Once};

use gtk::glib;
use sourceview5::{self as sourceview, prelude::*};

use crate::{config::ThemePreference, platform::assets::asset_path, ui::theme};

pub(crate) fn apply_language(buffer: &sourceview::Buffer, path: &Path) {
    ensure_sourceview_search_paths();
    let manager = sourceview::LanguageManager::default();
    let language = guess_language(&manager, path);
    buffer.set_language(language.as_ref());
}

pub(crate) fn apply_language_id(buffer: &sourceview::Buffer, language_id: &str) {
    ensure_sourceview_search_paths();
    let manager = sourceview::LanguageManager::default();
    buffer.set_language(manager.language(language_id).as_ref());
}

pub(crate) fn apply_style_scheme(buffer: &sourceview::Buffer, theme_preference: ThemePreference) {
    ensure_sourceview_search_paths();
    let manager = sourceview::StyleSchemeManager::default();
    let scheme = theme::sourceview_style_scheme_ids(theme_preference)
        .iter()
        .find_map(|scheme_id| manager.scheme(scheme_id));
    buffer.set_style_scheme(scheme.as_ref());
}

pub(crate) fn guess_language(
    manager: &sourceview::LanguageManager,
    path: &Path,
) -> Option<sourceview::Language> {
    if is_nginx_path(path) || looks_like_nginx_conf(path) {
        if let Some(language) = manager.language("nginx") {
            return Some(language);
        }
    }
    if is_vue_path(path) {
        if let Some(language) = manager.language("vue") {
            return Some(language);
        }
    }
    manager.guess_language(Some(path), None::<&str>)
}

pub(crate) fn has_guessable_language(path: &Path) -> bool {
    ensure_sourceview_search_paths();
    let manager = sourceview::LanguageManager::default();
    guess_language(&manager, path).is_some()
}

fn ensure_sourceview_search_paths() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        prepend_sourceview_dirs(
            sourceview::LanguageManager::default().search_path(),
            &[
                "assets/gtksourceview-5/language-specs",
                "share/gtksourceview-5/language-specs",
            ],
            |paths| sourceview::LanguageManager::default().set_search_path(paths),
        );
        prepend_sourceview_dirs(
            sourceview::StyleSchemeManager::default().search_path(),
            &[
                "assets/gtksourceview-5/styles",
                "share/gtksourceview-5/styles",
            ],
            |paths| sourceview::StyleSchemeManager::default().set_search_path(paths),
        );
    });
}

fn prepend_sourceview_dirs(
    current_paths: Vec<glib::GString>,
    relative_dirs: &[&str],
    set_paths: impl FnOnce(&[&str]),
) {
    let mut paths: Vec<String> = current_paths
        .into_iter()
        .map(|path| path.to_string())
        .collect();

    for relative in relative_dirs.iter().rev() {
        let candidate = asset_path(relative);
        if !candidate.is_dir() {
            continue;
        }
        let custom = candidate.to_string_lossy().into_owned();
        if paths.iter().any(|path| path == &custom) {
            continue;
        }
        paths.insert(0, custom);
    }

    let path_refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    set_paths(&path_refs);
}

fn is_vue_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("vue"))
}

fn is_nginx_path(path: &Path) -> bool {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if file_name == "nginx.conf" || file_name.ends_with(".nginx") {
        return true;
    }

    path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|part| part.eq_ignore_ascii_case("nginx"))
    })
}

fn looks_like_nginx_conf(path: &Path) -> bool {
    let is_conf = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("conf"));
    if !is_conf {
        return false;
    }

    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    let sample = String::from_utf8_lossy(&bytes[..bytes.len().min(16 * 1024)]);
    const MARKERS: &[&str] = &[
        "worker_processes",
        "server_name",
        "proxy_pass",
        "fastcgi_pass",
        "listen ",
        "location ",
        "upstream ",
        "http {",
        "events {",
    ];
    MARKERS
        .iter()
        .filter(|marker| sample.contains(*marker))
        .count()
        >= 2
}
