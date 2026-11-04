use std::rc::Rc;

use gtk::prelude::*;
use rust_i18n::t;

use crate::{
    config::{AppConfig, ThemePreference},
    i18n,
    ui::dsl::{button, dropdown, spin, Property},
};

use super::dialogs_base::{
    heading, hint, modal, paragraph, present_and_focus, row, switch_row, Once,
};

pub fn show_settings<F>(parent: &gtk::ApplicationWindow, current_config: AppConfig, on_save: F)
where
    F: FnOnce(AppConfig) + 'static,
{
    let model = Rc::new(SettingsModel::from_config(&current_config));
    let language_options = i18n::SUPPORTED_LOCALES
        .iter()
        .map(|locale| i18n::locale_display_name(locale))
        .collect::<Vec<_>>();
    let theme_options = [
        t!("settings.theme_system").into_owned(),
        t!("settings.theme_light").into_owned(),
        t!("settings.theme_dark").into_owned(),
    ];
    let theme_option_refs = theme_options.iter().map(String::as_str).collect::<Vec<_>>();
    let on_save = Once::new(on_save);

    let (window, save_button) = modal(parent, &t!("settings.title"), (620, 520), |dialog| {
        dialog.content(|| {
            heading(t!("settings.title"));
            paragraph(t!("settings.description"));
            hint(t!("settings.restart_hint"));

            heading(t!("settings.section_general"));
            row(t!("settings.language"), || {
                dropdown(&language_options, &model.language, |_| {});
            });
            row(t!("settings.theme"), || {
                dropdown(&theme_option_refs, &model.theme, |_| {});
            });

            heading(t!("settings.section_view"));
            switch_row(t!("settings.show_hidden_files"), &model.show_hidden_files);
            switch_row(t!("settings.folders_first"), &model.folders_first);

            heading(t!("settings.section_file_operations"));
            switch_row(t!("settings.use_recycle_bin"), &model.use_recycle_bin);
            switch_row(t!("settings.confirm_delete"), &model.confirm_delete);
            switch_row(t!("settings.confirm_overwrite"), &model.confirm_overwrite);

            heading(t!("settings.section_viewer"));
            row(t!("settings.streaming_threshold_mb"), || {
                spin(&model.streaming_threshold_mb, 1.0, 4096.0, 1.0, |_| {});
            });
            switch_row(t!("settings.line_wrap"), &model.line_wrap);
            switch_row(t!("settings.show_line_numbers"), &model.show_line_numbers);
        });

        dialog.actions(|| {
            button(t!("settings.reset_defaults"), |button| {
                let model = Rc::clone(&model);
                button.on_click(move || model.reset());
            });
            dialog.close_button(t!("common.cancel"));
            button(t!("common.save"), |button| {
                button.css("suggested-action");
                let model = Rc::clone(&model);
                let window = dialog.window().widget().clone();
                let on_save = on_save.clone();
                let current_config = current_config.clone();
                button.on_click(move || {
                    let mut next_config = current_config.clone();
                    model.apply_to(&mut next_config);
                    on_save.call(next_config);
                    window.close();
                });
            })
        })
    });
    window.set_default_widget(Some(save_button.widget()));

    present_and_focus(&window, save_button.widget());
}

/// The dialog's editable state. Widgets bind to these properties both ways,
/// so "Reset defaults" only touches the model and the save handler only reads it.
struct SettingsModel {
    language: Property<u32>,
    theme: Property<u32>,
    show_hidden_files: Property<bool>,
    folders_first: Property<bool>,
    use_recycle_bin: Property<bool>,
    confirm_delete: Property<bool>,
    confirm_overwrite: Property<bool>,
    streaming_threshold_mb: Property<f64>,
    line_wrap: Property<bool>,
    show_line_numbers: Property<bool>,
}

impl SettingsModel {
    fn from_config(config: &AppConfig) -> Self {
        let defaults = AppConfig::default();
        Self {
            language: Property::with_default(language_index(config), default_language_index()),
            theme: Property::with_default(
                theme_to_index(config.general.theme),
                theme_to_index(defaults.general.theme),
            ),
            show_hidden_files: Property::with_default(
                config.panels.show_hidden_files,
                defaults.panels.show_hidden_files,
            ),
            folders_first: Property::with_default(
                config.panels.folders_first,
                defaults.panels.folders_first,
            ),
            use_recycle_bin: Property::with_default(
                config.file_operations.use_recycle_bin,
                defaults.file_operations.use_recycle_bin,
            ),
            confirm_delete: Property::with_default(
                config.file_operations.confirm_delete,
                defaults.file_operations.confirm_delete,
            ),
            confirm_overwrite: Property::with_default(
                config.file_operations.confirm_overwrite,
                defaults.file_operations.confirm_overwrite,
            ),
            streaming_threshold_mb: Property::with_default(
                config.viewer.streaming_threshold_mb as f64,
                defaults.viewer.streaming_threshold_mb as f64,
            ),
            line_wrap: Property::with_default(config.viewer.line_wrap, defaults.viewer.line_wrap),
            show_line_numbers: Property::with_default(
                config.viewer.show_line_numbers,
                defaults.viewer.show_line_numbers,
            ),
        }
    }

    fn reset(&self) {
        self.language.reset();
        self.theme.reset();
        self.show_hidden_files.reset();
        self.folders_first.reset();
        self.use_recycle_bin.reset();
        self.confirm_delete.reset();
        self.confirm_overwrite.reset();
        self.streaming_threshold_mb.reset();
        self.line_wrap.reset();
        self.show_line_numbers.reset();
    }

    fn apply_to(&self, config: &mut AppConfig) {
        config.locale.language = i18n::SUPPORTED_LOCALES
            .get(self.language.get() as usize)
            .map(|locale| (*locale).to_string());
        config.general.theme = index_to_theme(self.theme.get());
        config.panels.show_hidden_files = self.show_hidden_files.get();
        config.panels.folders_first = self.folders_first.get();
        config.file_operations.use_recycle_bin = self.use_recycle_bin.get();
        config.file_operations.confirm_delete = self.confirm_delete.get();
        config.file_operations.confirm_overwrite = self.confirm_overwrite.get();
        config.viewer.streaming_threshold_mb = self.streaming_threshold_mb.get() as u64;
        config.viewer.line_wrap = self.line_wrap.get();
        config.viewer.show_line_numbers = self.show_line_numbers.get();
    }
}

fn language_index(config: &AppConfig) -> u32 {
    let selected_language = config
        .locale
        .language
        .as_deref()
        .and_then(i18n::normalize_locale)
        .unwrap_or_else(|| i18n::apply_locale(None));
    i18n::SUPPORTED_LOCALES
        .iter()
        .position(|locale| *locale == selected_language)
        .unwrap_or(1) as u32
}

fn default_language_index() -> u32 {
    i18n::SUPPORTED_LOCALES
        .iter()
        .position(|locale| *locale == "en")
        .unwrap_or(1) as u32
}

fn theme_to_index(theme: ThemePreference) -> u32 {
    match theme {
        ThemePreference::System => 0,
        ThemePreference::Light => 1,
        ThemePreference::Dark => 2,
    }
}

fn index_to_theme(index: u32) -> ThemePreference {
    match index {
        1 => ThemePreference::Light,
        2 => ThemePreference::Dark,
        _ => ThemePreference::System,
    }
}
