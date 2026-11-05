use super::normalize_locale;

// The locale is named per call: `rust_i18n::set_locale` is process-wide, and
// the tests of everything else run in parallel and read it.
#[test]
fn resolves_translations_from_locale_files() {
    assert_eq!(t!("entry.folder", locale = "de"), "Ordner");
    assert_eq!(t!("entry.folder", locale = "en"), "Folder");
    assert_eq!(t!("entry.folder", locale = "fr"), "Dossier");
    assert_eq!(t!("entry.folder", locale = "ru"), "Папка");
}

#[test]
fn normalizes_system_locale_names() {
    assert_eq!(normalize_locale("de_DE.UTF-8"), Some("de"));
    assert_eq!(normalize_locale("ru-RU"), Some("ru"));
    assert_eq!(normalize_locale("xx"), None);
}
