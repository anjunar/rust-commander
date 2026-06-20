pub const SUPPORTED_LOCALES: [&str; 3] = ["de", "en", "fr"];

pub fn apply_locale(preferred: Option<&str>) -> &'static str {
    let locale = preferred
        .and_then(normalize_locale)
        .or_else(|| detect_system_locale().and_then(|locale| normalize_locale(&locale)))
        .unwrap_or("en");
    rust_i18n::set_locale(locale);
    locale
}

pub fn normalize_locale(locale: &str) -> Option<&'static str> {
    let normalized = locale
        .split('.')
        .next()
        .unwrap_or(locale)
        .replace('_', "-")
        .to_lowercase();

    SUPPORTED_LOCALES.into_iter().find(|candidate| {
        normalized == *candidate || normalized.starts_with(&format!("{candidate}-"))
    })
}

pub fn locale_display_name(locale: &str) -> &'static str {
    match locale {
        "de" => "Deutsch",
        "en" => "English",
        "fr" => "Francais",
        _ => "English",
    }
}

fn detect_system_locale() -> Option<String> {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .find_map(|key| std::env::var(key).ok())
}

#[cfg(test)]
mod tests {
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
}
