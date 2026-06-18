use std::{cell::RefCell, path::PathBuf, rc::Rc};

use gtk::{glib, prelude::*};
use rust_i18n::t;

use crate::{
    config::AppConfig,
    domain::operation::{
        ConflictResolution, FileOperationKind, FileOperationRequest, OperationConflict,
    },
    fs::{operations::progress_percent, reader::format_bytes},
    i18n, presentation,
};

pub(crate) struct ModalWindow {
    pub window: gtk::Window,
    pub content: gtk::Box,
    pub actions: gtk::Box,
}

pub(crate) fn build_modal_window(
    parent: &gtk::ApplicationWindow,
    title: &str,
    default_width: i32,
    default_height: i32,
) -> ModalWindow {
    let window = gtk::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title(title)
        .default_width(default_width)
        .default_height(default_height)
        .build();

    let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    actions.set_halign(gtk::Align::End);

    root.append(&content);
    root.append(&actions);
    window.set_child(Some(&root));

    ModalWindow {
        window,
        content,
        actions,
    }
}

pub fn show_error(parent: &gtk::ApplicationWindow, title: &str, detail: &str) {
    gtk::AlertDialog::builder()
        .modal(true)
        .message(title)
        .detail(detail)
        .buttons([t!("common.close").into_owned()])
        .cancel_button(0)
        .default_button(0)
        .build()
        .show(Some(parent));
}

pub fn confirm_operation<F>(
    parent: &gtk::ApplicationWindow,
    request: FileOperationRequest,
    on_confirm: F,
) where
    F: FnOnce(FileOperationRequest) + 'static,
{
    let source_label = source_label(&request);
    let target_label = request
        .target_directory
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "-".into());

    let (title, detail, confirm_label) = match request.kind {
        FileOperationKind::Copy => (
            t!("dialog.copy_confirmation_title").into_owned(),
            t!(
                "dialog.copy_confirmation_detail",
                source = source_label.as_str(),
                target = target_label.as_str()
            )
            .into_owned(),
            presentation::file_operation_label(&FileOperationKind::Copy),
        ),
        FileOperationKind::Move => (
            t!("dialog.move_confirmation_title").into_owned(),
            t!(
                "dialog.move_confirmation_detail",
                source = source_label.as_str(),
                target = target_label.as_str()
            )
            .into_owned(),
            presentation::file_operation_label(&FileOperationKind::Move),
        ),
        FileOperationKind::Delete => (
            t!("dialog.delete_confirmation_title").into_owned(),
            t!(
                "dialog.delete_confirmation_detail",
                source = source_label.as_str()
            )
            .into_owned(),
            presentation::file_operation_label(&FileOperationKind::Delete),
        ),
    };

    let dialog = gtk::AlertDialog::builder()
        .modal(true)
        .message(title)
        .detail(&detail)
        .buttons([t!("common.cancel").into_owned(), confirm_label])
        .cancel_button(0)
        .default_button(1)
        .build();

    dialog.choose(
        Some(parent),
        None::<&gtk::gio::Cancellable>,
        move |response| {
            if matches!(response, Ok(1)) {
                on_confirm(request);
            }
        },
    );
}

pub fn prompt_rename<F>(parent: &gtk::ApplicationWindow, current_name: String, on_confirm: F)
where
    F: FnOnce(String) + 'static,
{
    let ModalWindow {
        window,
        content,
        actions,
    } = build_modal_window(parent, &t!("dialog.rename_title"), 420, 120);

    let label = gtk::Label::new(Some(&t!("dialog.rename_prompt")));
    label.set_xalign(0.0);
    content.append(&label);

    let entry = gtk::Entry::new();
    entry.set_text(&current_name);
    entry.set_hexpand(true);
    content.append(&entry);

    let cancel_button = gtk::Button::with_label(&t!("common.cancel"));
    let confirm_button = gtk::Button::with_label(&t!("common.rename"));
    confirm_button.add_css_class("suggested-action");
    actions.append(&cancel_button);
    actions.append(&confirm_button);
    window.set_default_widget(Some(&confirm_button));

    let callback = std::rc::Rc::new(std::cell::RefCell::new(Some(on_confirm)));
    {
        let window = window.clone();
        cancel_button.connect_clicked(move |_| {
            window.close();
        });
    }
    {
        let window = window.clone();
        let entry = entry.clone();
        let callback = std::rc::Rc::clone(&callback);
        confirm_button.connect_clicked(move |_| {
            let value = entry.text().to_string();
            if let Some(on_confirm) = callback.borrow_mut().take() {
                on_confirm(value);
            }
            window.close();
        });
    }

    glib::idle_add_local_once(move || {
        window.present();
        entry.grab_focus();
        entry.select_region(0, -1);
    });
}

pub fn prompt_new_directory<F>(parent: &gtk::ApplicationWindow, on_confirm: F)
where
    F: FnOnce(String) + 'static,
{
    let ModalWindow {
        window,
        content,
        actions,
    } = build_modal_window(parent, &t!("dialog.mkdir_title"), 420, 120);

    let label = gtk::Label::new(Some(&t!("dialog.mkdir_prompt")));
    label.set_xalign(0.0);
    content.append(&label);

    let entry = gtk::Entry::new();
    entry.set_hexpand(true);
    content.append(&entry);

    let cancel_button = gtk::Button::with_label(&t!("common.cancel"));
    let confirm_button = gtk::Button::with_label(&t!("common.create"));
    confirm_button.add_css_class("suggested-action");
    actions.append(&cancel_button);
    actions.append(&confirm_button);
    window.set_default_widget(Some(&confirm_button));

    let callback = std::rc::Rc::new(std::cell::RefCell::new(Some(on_confirm)));
    {
        let window = window.clone();
        cancel_button.connect_clicked(move |_| {
            window.close();
        });
    }
    {
        let window = window.clone();
        let entry = entry.clone();
        let callback = std::rc::Rc::clone(&callback);
        confirm_button.connect_clicked(move |_| {
            let value = entry.text().to_string();
            if let Some(on_confirm) = callback.borrow_mut().take() {
                on_confirm(value);
            }
            window.close();
        });
    }

    glib::idle_add_local_once(move || {
        window.present();
        entry.grab_focus();
    });
}

pub fn prompt_unix_chmod<F>(
    parent: &gtk::ApplicationWindow,
    selected_paths: Vec<PathBuf>,
    on_confirm: F,
) where
    F: FnOnce(String, bool) + 'static,
{
    let ModalWindow {
        window,
        content,
        actions,
    } = build_modal_window(parent, &t!("dialog.chmod_title"), 460, 180);

    let label = gtk::Label::new(Some(&t!("dialog.chmod_prompt")));
    label.set_xalign(0.0);
    content.append(&label);

    let summary = gtk::Label::new(Some(&selected_paths_summary(&selected_paths)));
    summary.set_xalign(0.0);
    summary.set_wrap(true);
    summary.add_css_class("dim-label");
    content.append(&summary);

    let mode_entry = gtk::Entry::new();
    mode_entry.set_hexpand(true);
    mode_entry.set_placeholder_text(Some(&t!("dialog.chmod_placeholder")));
    content.append(&mode_entry);

    let recursive_switch = gtk::Switch::new();
    content.append(&switch_row(
        &t!("dialog.recursive_apply"),
        &recursive_switch,
    ));

    let cancel_button = gtk::Button::with_label(&t!("common.cancel"));
    let confirm_button = gtk::Button::with_label(&t!("common.apply"));
    confirm_button.add_css_class("suggested-action");
    actions.append(&cancel_button);
    actions.append(&confirm_button);
    window.set_default_widget(Some(&confirm_button));

    let callback = Rc::new(RefCell::new(Some(on_confirm)));
    {
        let window = window.clone();
        cancel_button.connect_clicked(move |_| {
            window.close();
        });
    }
    {
        let window = window.clone();
        let mode_entry = mode_entry.clone();
        let recursive_switch = recursive_switch.clone();
        let callback = Rc::clone(&callback);
        confirm_button.connect_clicked(move |_| {
            let mode = mode_entry.text().to_string();
            let recursive = recursive_switch.is_active();
            if let Some(on_confirm) = callback.borrow_mut().take() {
                on_confirm(mode, recursive);
            }
            window.close();
        });
    }

    glib::idle_add_local_once(move || {
        window.present();
        mode_entry.grab_focus();
    });
}

pub fn prompt_unix_chown<F>(
    parent: &gtk::ApplicationWindow,
    selected_paths: Vec<PathBuf>,
    on_confirm: F,
) where
    F: FnOnce(String, bool) + 'static,
{
    let ModalWindow {
        window,
        content,
        actions,
    } = build_modal_window(parent, &t!("dialog.chown_title"), 460, 180);

    let label = gtk::Label::new(Some(&t!("dialog.chown_prompt")));
    label.set_xalign(0.0);
    content.append(&label);

    let summary = gtk::Label::new(Some(&selected_paths_summary(&selected_paths)));
    summary.set_xalign(0.0);
    summary.set_wrap(true);
    summary.add_css_class("dim-label");
    content.append(&summary);

    let owner_entry = gtk::Entry::new();
    owner_entry.set_hexpand(true);
    owner_entry.set_placeholder_text(Some(&t!("dialog.chown_placeholder")));
    content.append(&owner_entry);

    let recursive_switch = gtk::Switch::new();
    content.append(&switch_row(
        &t!("dialog.recursive_apply"),
        &recursive_switch,
    ));

    let cancel_button = gtk::Button::with_label(&t!("common.cancel"));
    let confirm_button = gtk::Button::with_label(&t!("common.apply"));
    confirm_button.add_css_class("suggested-action");
    actions.append(&cancel_button);
    actions.append(&confirm_button);
    window.set_default_widget(Some(&confirm_button));

    let callback = Rc::new(RefCell::new(Some(on_confirm)));
    {
        let window = window.clone();
        cancel_button.connect_clicked(move |_| {
            window.close();
        });
    }
    {
        let window = window.clone();
        let owner_entry = owner_entry.clone();
        let recursive_switch = recursive_switch.clone();
        let callback = Rc::clone(&callback);
        confirm_button.connect_clicked(move |_| {
            let owner = owner_entry.text().to_string();
            let recursive = recursive_switch.is_active();
            if let Some(on_confirm) = callback.borrow_mut().take() {
                on_confirm(owner, recursive);
            }
            window.close();
        });
    }

    glib::idle_add_local_once(move || {
        window.present();
        owner_entry.grab_focus();
    });
}

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

pub fn show_conflict<F>(
    parent: &gtk::ApplicationWindow,
    conflict: OperationConflict,
    on_resolution: F,
) where
    F: FnOnce(ConflictResolution) + 'static,
{
    let detail = format!(
        "{}",
        t!(
            "dialog.conflict_detail",
            source = conflict.source.display().to_string(),
            target = conflict.target.display().to_string()
        )
    );
    let dialog = gtk::AlertDialog::builder()
        .modal(true)
        .message(
            t!(
                "dialog.conflict_title",
                kind = presentation::file_operation_label(&conflict.kind)
            )
            .into_owned(),
        )
        .detail(&detail)
        .buttons([
            t!("common.cancel").into_owned(),
            t!("common.skip").into_owned(),
            t!("common.rename").into_owned(),
            t!("common.overwrite").into_owned(),
        ])
        .cancel_button(0)
        .default_button(1)
        .build();

    dialog.choose(
        Some(parent),
        None::<&gtk::gio::Cancellable>,
        move |response| {
            let resolution = match response {
                Ok(3) => ConflictResolution::Overwrite,
                Ok(1) => ConflictResolution::Skip,
                Ok(2) => ConflictResolution::Rename,
                _ => ConflictResolution::Cancel,
            };
            on_resolution(resolution);
        },
    );
}

#[derive(Clone)]
pub struct ProgressDialog {
    window: gtk::Window,
    title: gtk::Label,
    detail: gtk::Label,
    eta: gtk::Label,
    progress: gtk::ProgressBar,
}

impl ProgressDialog {
    pub fn new<F>(parent: &gtk::ApplicationWindow, title_text: &str, on_cancel: F) -> Self
    where
        F: Fn() + 'static,
    {
        let ModalWindow {
            window,
            content,
            actions,
        } = build_modal_window(parent, title_text, 460, 160);

        let title = gtk::Label::new(Some(title_text));
        title.set_xalign(0.0);
        title.add_css_class("dialog-title");
        content.append(&title);

        let detail = gtk::Label::new(Some(&t!("progress.preparing_file_operation")));
        detail.set_xalign(0.0);
        detail.set_wrap(true);
        content.append(&detail);

        let progress = gtk::ProgressBar::new();
        progress.set_show_text(true);
        content.append(&progress);

        let eta = gtk::Label::new(Some(&t!("progress.eta_unknown")));
        eta.set_xalign(0.0);
        content.append(&eta);

        let cancel_button = gtk::Button::with_label(&t!("progress.cancel_operation"));
        actions.append(&cancel_button);

        let window_for_cancel = window.clone();
        cancel_button.connect_clicked(move |_| {
            on_cancel();
            window_for_cancel.set_sensitive(false);
        });

        window.present();

        Self {
            window,
            title,
            detail,
            eta,
            progress,
        }
    }

    pub fn update_progress(&self, snapshot: &crate::domain::operation::OperationSnapshot) {
        let fraction = progress_percent(snapshot);
        self.title.set_label(&t!(
            "progress.in_progress",
            kind = presentation::file_operation_label(&snapshot.kind)
        ));
        self.detail.set_label(&t!(
            "progress.file_operation_detail",
            processed = format_bytes(snapshot.processed_bytes),
            total = format_bytes(snapshot.total_bytes),
            processed_entries = snapshot.processed_entries,
            total_entries = snapshot.total_entries,
            item = snapshot.current_item.as_str()
        ));
        self.progress.set_fraction(fraction);
        self.progress
            .set_text(Some(&format!("{:.0}%", fraction * 100.0)));
        self.eta
            .set_label(&crate::fs::operations::format_eta(snapshot));
    }

    pub fn update_archive_progress(&self, progress: &crate::archive::ArchiveProgress) {
        self.title
            .set_label(&t!("progress.archive_copy_in_progress"));

        let detail = progress.current_path.clone().unwrap_or_else(|| {
            progress
                .operation
                .as_ref()
                .map(|operation| match operation {
                    crate::archive::ArchiveOperation::ExtractEntry { entry_path, .. } => {
                        entry_path.clone()
                    }
                    crate::archive::ArchiveOperation::ExtractEntries { entry_paths, .. } => {
                        t!("progress.selected_archive_items", count = entry_paths.len())
                            .into_owned()
                    }
                    crate::archive::ArchiveOperation::ExtractAll { .. } => {
                        t!("progress.extracting_complete_archive").into_owned()
                    }
                    crate::archive::ArchiveOperation::OpenArchive => {
                        t!("progress.opening_archive").into_owned()
                    }
                    crate::archive::ArchiveOperation::List => {
                        t!("progress.listing_archive").into_owned()
                    }
                    crate::archive::ArchiveOperation::Test => {
                        t!("progress.testing_archive").into_owned()
                    }
                })
                .unwrap_or_else(|| t!("progress.preparing_archive_operation").into_owned())
        });
        self.detail.set_label(&detail);

        if let Some(percent) = progress.percent {
            let clamped = percent.clamp(0.0, 1.0);
            self.progress.set_fraction(clamped);
            self.progress
                .set_text(Some(&format!("{:.0}%", clamped * 100.0)));
        } else {
            self.progress.pulse();
            self.progress.set_text(Some(&t!("progress.working")));
        }

        let processed_entries = progress.processed_entries.unwrap_or(0);
        let total_entries = progress.total_entries.unwrap_or(0);
        self.eta.set_label(&t!(
            "progress.items_count",
            processed = processed_entries,
            total = total_entries
        ));
    }

    pub fn set_waiting_for_conflict(&self) {
        self.detail
            .set_label(&t!("progress.waiting_for_conflict_resolution"));
        self.progress.pulse();
    }

    pub fn close(&self) {
        self.window.close();
    }
}

fn source_label(request: &FileOperationRequest) -> String {
    if let Some(archive_source) = &request.archive_source {
        return if archive_source.entry_paths.len() == 1 {
            archive_source.entry_paths[0].clone()
        } else {
            t!(
                "progress.selected_archive_items",
                count = archive_source.entry_paths.len()
            )
            .into_owned()
        };
    }

    if request.sources.len() == 1 {
        request
            .sources
            .first()
            .and_then(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| request.sources[0].display().to_string())
    } else {
        t!("common.items_count", count = request.sources.len()).into_owned()
    }
}

fn selected_paths_summary(paths: &[PathBuf]) -> String {
    if paths.len() == 1 {
        return paths[0].display().to_string();
    }

    t!("common.items_count", count = paths.len()).into_owned()
}
