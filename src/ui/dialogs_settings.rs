use std::{cell::RefCell, rc::Rc};

use gtk::{glib, prelude::*};
use rust_i18n::t;

use crate::{config::AppConfig, i18n};

use super::{build_modal_window, dialogs_base::ModalWindow};

pub fn show_settings<F>(parent: &gtk::ApplicationWindow, current_config: AppConfig, on_save: F)
where
    F: FnOnce(AppConfig) + 'static,
{
    let ModalWindow {
        window,
        content,
        actions,
    } = build_modal_window(parent, &t!("settings.title"), 620, 520);

    let title = gtk::Label::new(Some(&t!("settings.title")));
    title.set_xalign(0.0);
    title.add_css_class("dialog-title");
    content.append(&title);

    let description = gtk::Label::new(Some(&t!("settings.description")));
    description.set_xalign(0.0);
    description.set_wrap(true);
    content.append(&description);

    let info = gtk::Label::new(Some(&t!("settings.restart_hint")));
    info.set_xalign(0.0);
    info.set_wrap(true);
    info.add_css_class("dim-label");
    content.append(&info);

    let general_section = section_label(&t!("settings.section_general"));
    content.append(&general_section);

    let language_row = settings_row();
    let language_label = row_label(&t!("settings.language"));
    let language_options = i18n::SUPPORTED_LOCALES
        .iter()
        .map(|locale| i18n::locale_display_name(locale))
        .collect::<Vec<_>>();
    let language_model = gtk::StringList::new(&language_options);
    let language_dropdown = gtk::DropDown::new(Some(language_model.clone()), gtk::Expression::NONE);
    let selected_language = current_config
        .locale
        .language
        .as_deref()
        .and_then(i18n::normalize_locale)
        .unwrap_or_else(|| i18n::apply_locale(None));
    let selected_index = i18n::SUPPORTED_LOCALES
        .iter()
        .position(|locale| *locale == selected_language)
        .unwrap_or(1);
    language_dropdown.set_selected(selected_index as u32);
    language_row.append(&language_label);
    language_row.append(&language_dropdown);
    content.append(&language_row);

    let theme_row = settings_row();
    let theme_label = row_label(&t!("settings.theme"));
    let theme_options = [
        t!("settings.theme_system").into_owned(),
        t!("settings.theme_light").into_owned(),
        t!("settings.theme_dark").into_owned(),
    ];
    let theme_option_refs = theme_options.iter().map(String::as_str).collect::<Vec<_>>();
    let theme_model = gtk::StringList::new(&theme_option_refs);
    let theme_dropdown = gtk::DropDown::new(Some(theme_model.clone()), gtk::Expression::NONE);
    theme_dropdown.set_selected(theme_to_index(current_config.general.theme) as u32);
    theme_row.append(&theme_label);
    theme_row.append(&theme_dropdown);
    content.append(&theme_row);

    let view_section = section_label(&t!("settings.section_view"));
    content.append(&view_section);

    let show_hidden_switch = gtk::Switch::builder()
        .active(current_config.panels.show_hidden_files)
        .build();
    content.append(&switch_row(
        &t!("settings.show_hidden_files"),
        &show_hidden_switch,
    ));

    let folders_first_switch = gtk::Switch::builder()
        .active(current_config.panels.folders_first)
        .build();
    content.append(&switch_row(
        &t!("settings.folders_first"),
        &folders_first_switch,
    ));

    let file_operations_section = section_label(&t!("settings.section_file_operations"));
    content.append(&file_operations_section);

    let use_recycle_bin_switch = gtk::Switch::builder()
        .active(current_config.file_operations.use_recycle_bin)
        .build();
    content.append(&switch_row(
        &t!("settings.use_recycle_bin"),
        &use_recycle_bin_switch,
    ));

    let confirm_delete_switch = gtk::Switch::builder()
        .active(current_config.file_operations.confirm_delete)
        .build();
    content.append(&switch_row(
        &t!("settings.confirm_delete"),
        &confirm_delete_switch,
    ));

    let confirm_overwrite_switch = gtk::Switch::builder()
        .active(current_config.file_operations.confirm_overwrite)
        .build();
    content.append(&switch_row(
        &t!("settings.confirm_overwrite"),
        &confirm_overwrite_switch,
    ));

    let viewer_section = section_label(&t!("settings.section_viewer"));
    content.append(&viewer_section);

    let threshold_row = settings_row();
    let threshold_label = row_label(&t!("settings.streaming_threshold_mb"));
    let threshold_adjustment = gtk::Adjustment::new(
        current_config.viewer.streaming_threshold_mb as f64,
        1.0,
        4096.0,
        1.0,
        10.0,
        0.0,
    );
    let threshold_input = gtk::SpinButton::new(Some(&threshold_adjustment), 1.0, 0);
    threshold_row.append(&threshold_label);
    threshold_row.append(&threshold_input);
    content.append(&threshold_row);

    let line_wrap_switch = gtk::Switch::builder()
        .active(current_config.viewer.line_wrap)
        .build();
    content.append(&switch_row(&t!("settings.line_wrap"), &line_wrap_switch));

    let show_line_numbers_switch = gtk::Switch::builder()
        .active(current_config.viewer.show_line_numbers)
        .build();
    content.append(&switch_row(
        &t!("settings.show_line_numbers"),
        &show_line_numbers_switch,
    ));

    let reset_button = gtk::Button::with_label(&t!("settings.reset_defaults"));
    let cancel_button = gtk::Button::with_label(&t!("common.cancel"));
    let save_button = gtk::Button::with_label(&t!("common.save"));
    save_button.add_css_class("suggested-action");
    actions.append(&reset_button);
    actions.append(&cancel_button);
    actions.append(&save_button);
    window.set_default_widget(Some(&save_button));

    {
        let language_dropdown = language_dropdown.clone();
        let theme_dropdown = theme_dropdown.clone();
        let show_hidden_switch = show_hidden_switch.clone();
        let folders_first_switch = folders_first_switch.clone();
        let use_recycle_bin_switch = use_recycle_bin_switch.clone();
        let confirm_delete_switch = confirm_delete_switch.clone();
        let confirm_overwrite_switch = confirm_overwrite_switch.clone();
        let threshold_input = threshold_input.clone();
        let line_wrap_switch = line_wrap_switch.clone();
        let show_line_numbers_switch = show_line_numbers_switch.clone();
        reset_button.connect_clicked(move |_| {
            let defaults = AppConfig::default();
            language_dropdown.set_selected(default_language_index() as u32);
            theme_dropdown.set_selected(theme_to_index(defaults.general.theme) as u32);
            show_hidden_switch.set_active(defaults.panels.show_hidden_files);
            folders_first_switch.set_active(defaults.panels.folders_first);
            use_recycle_bin_switch.set_active(defaults.file_operations.use_recycle_bin);
            confirm_delete_switch.set_active(defaults.file_operations.confirm_delete);
            confirm_overwrite_switch.set_active(defaults.file_operations.confirm_overwrite);
            threshold_input.set_value(defaults.viewer.streaming_threshold_mb as f64);
            line_wrap_switch.set_active(defaults.viewer.line_wrap);
            show_line_numbers_switch.set_active(defaults.viewer.show_line_numbers);
        });
    }

    {
        let window = window.clone();
        cancel_button.connect_clicked(move |_| {
            window.close();
        });
    }

    let callback = Rc::new(RefCell::new(Some(on_save)));
    {
        let window = window.clone();
        let callback = Rc::clone(&callback);
        let language_dropdown = language_dropdown.clone();
        let theme_dropdown = theme_dropdown.clone();
        let show_hidden_switch = show_hidden_switch.clone();
        let folders_first_switch = folders_first_switch.clone();
        let use_recycle_bin_switch = use_recycle_bin_switch.clone();
        let confirm_delete_switch = confirm_delete_switch.clone();
        let confirm_overwrite_switch = confirm_overwrite_switch.clone();
        let threshold_input = threshold_input.clone();
        let line_wrap_switch = line_wrap_switch.clone();
        let show_line_numbers_switch = show_line_numbers_switch.clone();
        let current_config = current_config.clone();
        save_button.connect_clicked(move |_| {
            let mut next_config = current_config.clone();
            next_config.archive = current_config.archive.clone();
            next_config.locale.language = i18n::SUPPORTED_LOCALES
                .get(language_dropdown.selected() as usize)
                .map(|locale| (*locale).to_string());
            next_config.general.theme = index_to_theme(theme_dropdown.selected());
            next_config.panels.show_hidden_files = show_hidden_switch.is_active();
            next_config.panels.folders_first = folders_first_switch.is_active();
            next_config.file_operations.use_recycle_bin = use_recycle_bin_switch.is_active();
            next_config.file_operations.confirm_delete = confirm_delete_switch.is_active();
            next_config.file_operations.confirm_overwrite = confirm_overwrite_switch.is_active();
            next_config.viewer.streaming_threshold_mb = threshold_input.value() as u64;
            next_config.viewer.line_wrap = line_wrap_switch.is_active();
            next_config.viewer.show_line_numbers = show_line_numbers_switch.is_active();

            if let Some(on_save) = callback.borrow_mut().take() {
                on_save(next_config);
            }
            window.close();
        });
    }

    glib::idle_add_local_once(move || {
        window.present();
        save_button.grab_focus();
    });
}

fn settings_row() -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Horizontal, 8)
}

fn row_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label
}

fn section_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.add_css_class("dialog-title");
    label
}

fn switch_row(text: &str, switch: &gtk::Switch) -> gtk::Box {
    let row = settings_row();
    row.append(&row_label(text));
    row.append(switch);
    row
}

fn theme_to_index(theme: crate::config::ThemePreference) -> usize {
    match theme {
        crate::config::ThemePreference::System => 0,
        crate::config::ThemePreference::Light => 1,
        crate::config::ThemePreference::Dark => 2,
    }
}

fn index_to_theme(index: u32) -> crate::config::ThemePreference {
    match index {
        1 => crate::config::ThemePreference::Light,
        2 => crate::config::ThemePreference::Dark,
        _ => crate::config::ThemePreference::System,
    }
}

fn default_language_index() -> usize {
    i18n::SUPPORTED_LOCALES
        .iter()
        .position(|locale| *locale == "en")
        .unwrap_or(1)
}
