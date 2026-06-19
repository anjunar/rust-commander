use crate::config::ThemePreference;

const BASE_CSS: &str = include_str!("theme_base.css");
const LIGHT_CSS: &str = include_str!("theme_light.css");
const DARK_CSS: &str = include_str!("theme_dark.css");

pub struct ThemeController {
    provider: gtk::CssProvider,
}

impl ThemeController {
    pub fn new() -> Self {
        let provider = gtk::CssProvider::new();
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }

        Self { provider }
    }

    pub fn apply(&self, preference: ThemePreference) {
        self.provider
            .load_from_string(&format!("{BASE_CSS}\n{}", palette_css(preference)));
    }
}

fn palette_css(preference: ThemePreference) -> &'static str {
    match resolved_variant(preference) {
        ThemeVariant::Light => LIGHT_CSS,
        ThemeVariant::Dark => DARK_CSS,
    }
}

fn resolved_variant(preference: ThemePreference) -> ThemeVariant {
    match preference {
        ThemePreference::Light => ThemeVariant::Light,
        ThemePreference::Dark => ThemeVariant::Dark,
        ThemePreference::System => {
            let prefers_dark = gtk::Settings::default()
                .map(|settings| settings.is_gtk_application_prefer_dark_theme())
                .unwrap_or(false);
            if prefers_dark {
                ThemeVariant::Dark
            } else {
                ThemeVariant::Light
            }
        }
    }
}

enum ThemeVariant {
    Light,
    Dark,
}
