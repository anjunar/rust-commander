use super::apply_locale;

#[test]
fn resolves_translations_from_locale_files() {
    apply_locale(Some("de"));
    assert_eq!(t!("entry.folder"), "Ordner");

    apply_locale(Some("en"));
    assert_eq!(t!("entry.folder"), "Folder");

    apply_locale(Some("fr"));
    assert_eq!(t!("entry.folder"), "Dossier");
}
