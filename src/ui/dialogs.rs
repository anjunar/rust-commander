use std::{cell::RefCell, path::PathBuf, rc::Rc};

use gtk::{glib, prelude::*};
use rust_i18n::t;

use crate::{
    application::{
        ConflictResolution, FileOperationKind, OperationConflict, OperationPlan,
    },
    config::AppConfig,
    fs::{operations::progress_percent, reader::format_bytes},
    i18n, presentation,
    remote::{RemoteAuthConfig, RemoteConfig, RemoteProfile, RemoteRuntimeSecret, RemoteSession},
};

#[derive(Clone)]
pub struct RemoteConnectionDialogResult {
    pub session: RemoteSession,
    pub last_used_profile: Option<String>,
}

#[derive(Clone)]
pub enum RemoteDialogAction {
    Connect(RemoteConnectionDialogResult),
    SaveProfile {
        profile: RemoteProfile,
        previous_name: Option<String>,
    },
    DeleteProfile {
        name: String,
    },
}

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

    {
        let parent = parent.clone();
        window.connect_close_request(move |_| {
            let parent = parent.clone();
            glib::idle_add_local_once(move || {
                parent.present();
                parent.grab_focus();
            });
            glib::Propagation::Proceed
        });
    }

    #[cfg(target_os = "windows")]
    install_dialog_window_controls(&window, title);

    let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
    root.set_margin_top(12);
    root.set_margin_bottom(14);
    root.set_margin_start(12);
    root.set_margin_end(12);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    actions.set_halign(gtk::Align::End);
    actions.set_margin_top(3);
    actions.set_margin_bottom(0);

    root.append(&content);
    root.append(&actions);
    window.set_child(Some(&root));

    ModalWindow {
        window,
        content,
        actions,
    }
}

#[cfg(target_os = "windows")]
fn install_dialog_window_controls(window: &gtk::Window, title: &str) {
    let header = gtk::HeaderBar::new();
    header.set_show_title_buttons(false);

    let title_label = gtk::Label::new(Some(title));
    title_label.add_css_class("app-title");
    header.set_title_widget(Some(&title_label));

    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    controls.add_css_class("window-controls");

    let close_button = gtk::Button::from_icon_name("window-close-symbolic");
    close_button.add_css_class("window-control-button");
    close_button.add_css_class("window-close-button");
    close_button.add_css_class("flat");
    close_button.set_focus_on_click(false);
    close_button.set_size_request(44, 28);
    close_button.set_tooltip_text(Some("Close"));
    {
        let window = window.clone();
        close_button.connect_clicked(move |_| {
            window.close();
        });
    }
    controls.append(&close_button);

    header.pack_end(&controls);
    window.set_titlebar(Some(&header));
}

pub fn show_error(parent: &gtk::ApplicationWindow, title: &str, detail: &str) {
    let ModalWindow {
        window,
        content,
        actions,
    } = build_modal_window(parent, title, 460, 180);

    let title_label = gtk::Label::new(Some(title));
    title_label.set_xalign(0.0);
    title_label.set_wrap(true);
    title_label.add_css_class("dialog-title");
    content.append(&title_label);

    let detail_label = gtk::Label::new(Some(detail));
    detail_label.set_xalign(0.0);
    detail_label.set_wrap(true);
    content.append(&detail_label);

    let close_button = gtk::Button::with_label(&t!("common.close"));
    close_button.add_css_class("suggested-action");
    actions.append(&close_button);
    window.set_default_widget(Some(&close_button));

    let window_for_close = window.clone();
    close_button.connect_clicked(move |_| {
        window_for_close.close();
    });

    window.present();
}

pub fn prompt_archive_open_action<F>(
    parent: &gtk::ApplicationWindow,
    archive_path: PathBuf,
    on_choice: F,
) where
    F: FnOnce(bool) + 'static,
{
    let title = t!("dialog.archive_open_title").into_owned();
    let detail = t!(
        "dialog.archive_open_detail",
        path = archive_path.display().to_string()
    )
    .into_owned();

    let ModalWindow {
        window,
        content,
        actions,
    } = build_modal_window(parent, &title, 500, 180);

    let title_label = gtk::Label::new(Some(&title));
    title_label.set_xalign(0.0);
    title_label.set_wrap(true);
    title_label.add_css_class("dialog-title");
    content.append(&title_label);

    let detail_label = gtk::Label::new(Some(&detail));
    detail_label.set_xalign(0.0);
    detail_label.set_wrap(true);
    content.append(&detail_label);

    let cancel_button = gtk::Button::with_label(&t!("common.cancel"));
    let external_button = gtk::Button::with_label(&t!("dialog.archive_open_external"));
    let archive_button = gtk::Button::with_label(&t!("dialog.archive_open_internal"));
    archive_button.add_css_class("suggested-action");
    actions.append(&cancel_button);
    actions.append(&external_button);
    actions.append(&archive_button);
    window.set_default_widget(Some(&archive_button));

    {
        let window = window.clone();
        cancel_button.connect_clicked(move |_| {
            window.close();
        });
    }

    let callback = Rc::new(RefCell::new(Some(on_choice)));
    {
        let window = window.clone();
        let callback = Rc::clone(&callback);
        archive_button.connect_clicked(move |_| {
            if let Some(on_choice) = callback.borrow_mut().take() {
                on_choice(true);
            }
            window.close();
        });
    }
    {
        let window = window.clone();
        let callback = Rc::clone(&callback);
        external_button.connect_clicked(move |_| {
            if let Some(on_choice) = callback.borrow_mut().take() {
                on_choice(false);
            }
            window.close();
        });
    }

    window.present();
}

pub fn confirm_operation<F>(
    parent: &gtk::ApplicationWindow,
    request: OperationPlan,
    on_confirm: F,
) where
    F: FnOnce(OperationPlan) + 'static,
{
    let source_label = source_label(&request);
    let target_label = target_label(&request);

    let (title, detail, confirm_label) = match request.kind() {
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

    let ModalWindow {
        window,
        content,
        actions,
    } = build_modal_window(parent, &title, 480, 160);

    let title_label = gtk::Label::new(Some(&title));
    title_label.set_xalign(0.0);
    title_label.set_wrap(true);
    title_label.add_css_class("dialog-title");
    content.append(&title_label);

    let detail_label = gtk::Label::new(Some(&detail));
    detail_label.set_xalign(0.0);
    detail_label.set_wrap(true);
    content.append(&detail_label);

    let cancel_button = gtk::Button::with_label(&t!("common.cancel"));
    let confirm_button = gtk::Button::with_label(&confirm_label);
    confirm_button.add_css_class("suggested-action");
    actions.append(&cancel_button);
    actions.append(&confirm_button);
    window.set_default_widget(Some(&confirm_button));

    {
        let window = window.clone();
        cancel_button.connect_clicked(move |_| {
            window.close();
        });
    }

    let callback = Rc::new(RefCell::new(Some(on_confirm)));
    let request = Rc::new(RefCell::new(Some(request)));
    {
        let window = window.clone();
        let callback = Rc::clone(&callback);
        let request = Rc::clone(&request);
        confirm_button.connect_clicked(move |_| {
            window.close();
            if let (Some(on_confirm), Some(request)) =
                (callback.borrow_mut().take(), request.borrow_mut().take())
            {
                glib::idle_add_local_once(move || {
                    on_confirm(request);
                });
            }
        });
    }

    window.present();
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

pub fn prompt_remote_connection<F>(
    parent: &gtk::ApplicationWindow,
    remote_config: RemoteConfig,
    on_action: F,
) where
    F: Fn(RemoteDialogAction) + 'static,
{
    let ModalWindow {
        window,
        content,
        actions,
    } = build_modal_window(parent, "Connect To Remote", 520, 470);

    let state = Rc::new(RefCell::new(remote_config));
    let callback: Rc<dyn Fn(RemoteDialogAction)> = Rc::new(on_action);

    let picker_row = settings_row();
    let picker_label = row_label("Saved profile");
    let profile_model = gtk::StringList::new(&[]);
    let profile_dropdown = gtk::DropDown::new(Some(profile_model.clone()), gtk::Expression::NONE);
    picker_row.append(&picker_label);
    picker_row.append(&profile_dropdown);
    content.append(&picker_row);

    let profile_name_entry = gtk::Entry::new();
    profile_name_entry.set_placeholder_text(Some("Profile name"));
    content.append(&profile_name_entry);

    let host_entry = gtk::Entry::new();
    host_entry.set_placeholder_text(Some("Host"));
    content.append(&host_entry);

    let port_row = settings_row();
    let port_label = row_label("Port");
    let port_adjustment = gtk::Adjustment::new(22.0, 1.0, 65535.0, 1.0, 10.0, 0.0);
    let port_input = gtk::SpinButton::new(Some(&port_adjustment), 1.0, 0);
    port_row.append(&port_label);
    port_row.append(&port_input);
    content.append(&port_row);

    let username_entry = gtk::Entry::new();
    username_entry.set_placeholder_text(Some("Username"));
    content.append(&username_entry);

    let start_directory_entry = gtk::Entry::new();
    start_directory_entry.set_placeholder_text(Some("Start directory"));
    start_directory_entry.set_text("/");
    content.append(&start_directory_entry);

    let skip_host_key_verification_switch = gtk::Switch::builder().active(false).build();
    content.append(&switch_row(
        "Insecure: skip host key verification",
        &skip_host_key_verification_switch,
    ));

    let host_key_warning = gtk::Label::new(Some(
        "Only use this for test or demo servers. This disables known_hosts validation and allows man-in-the-middle attacks.",
    ));
    host_key_warning.set_xalign(0.0);
    host_key_warning.set_wrap(true);
    host_key_warning.add_css_class("dim-label");
    content.append(&host_key_warning);

    let auth_row = settings_row();
    let auth_label = row_label("Authentication");
    let auth_model = gtk::StringList::new(&["Password", "Key file"]);
    let auth_dropdown = gtk::DropDown::new(Some(auth_model.clone()), gtk::Expression::NONE);
    auth_row.append(&auth_label);
    auth_row.append(&auth_dropdown);
    content.append(&auth_row);

    let password_entry = gtk::PasswordEntry::new();
    password_entry.set_placeholder_text(Some("Password or passphrase"));
    content.append(&password_entry);

    let private_key_entry = gtk::Entry::new();
    private_key_entry.set_placeholder_text(Some("Private key path"));
    content.append(&private_key_entry);

    let public_key_entry = gtk::Entry::new();
    public_key_entry.set_placeholder_text(Some("Public key path (optional)"));
    content.append(&public_key_entry);

    let save_profile_switch = gtk::Switch::builder().active(false).build();
    content.append(&switch_row(
        "Save profile without secret",
        &save_profile_switch,
    ));

    let delete_profile_button = gtk::Button::with_label("Delete profile");
    delete_profile_button.add_css_class("destructive-action");
    content.append(&delete_profile_button);

    let update_auth_visibility: Rc<dyn Fn(bool)> = Rc::new({
        let password_entry = password_entry.clone();
        let private_key_entry = private_key_entry.clone();
        let public_key_entry = public_key_entry.clone();
        move |is_password: bool| {
            password_entry.set_visible(true);
            private_key_entry.set_visible(!is_password);
            public_key_entry.set_visible(!is_password);
            private_key_entry.set_sensitive(!is_password);
            public_key_entry.set_sensitive(!is_password);
        }
    });
    update_auth_visibility(true);

    let populate_form: Rc<dyn Fn(Option<&RemoteProfile>)> = Rc::new({
        let profile_name_entry = profile_name_entry.clone();
        let host_entry = host_entry.clone();
        let port_input = port_input.clone();
        let username_entry = username_entry.clone();
        let start_directory_entry = start_directory_entry.clone();
        let skip_host_key_verification_switch = skip_host_key_verification_switch.clone();
        let auth_dropdown = auth_dropdown.clone();
        let password_entry = password_entry.clone();
        let private_key_entry = private_key_entry.clone();
        let public_key_entry = public_key_entry.clone();
        let save_profile_switch = save_profile_switch.clone();
        let delete_profile_button = delete_profile_button.clone();
        move |profile| {
            if let Some(profile) = profile {
                profile_name_entry.set_text(&profile.name);
                host_entry.set_text(&profile.host);
                port_input.set_value(profile.port as f64);
                username_entry.set_text(profile.auth.username());
                start_directory_entry.set_text(profile.start_directory.as_str());
                skip_host_key_verification_switch.set_active(profile.skip_host_key_verification);
                match &profile.auth {
                    RemoteAuthConfig::Password { .. } => {
                        auth_dropdown.set_selected(0);
                        private_key_entry.set_text("");
                        public_key_entry.set_text("");
                    }
                    RemoteAuthConfig::KeyFile {
                        private_key_path,
                        public_key_path,
                        ..
                    } => {
                        auth_dropdown.set_selected(1);
                        private_key_entry.set_text(&private_key_path.display().to_string());
                        public_key_entry.set_text(
                            &public_key_path
                                .as_ref()
                                .map(|path| path.display().to_string())
                                .unwrap_or_default(),
                        );
                    }
                }
                password_entry.set_text("");
                save_profile_switch.set_active(true);
                delete_profile_button.set_sensitive(true);
            } else {
                profile_name_entry.set_text("");
                host_entry.set_text("");
                port_input.set_value(22.0);
                username_entry.set_text("");
                start_directory_entry.set_text("/");
                skip_host_key_verification_switch.set_active(false);
                auth_dropdown.set_selected(0);
                password_entry.set_text("");
                private_key_entry.set_text("");
                public_key_entry.set_text("");
                save_profile_switch.set_active(false);
                delete_profile_button.set_sensitive(false);
            }
        }
    });

    let refresh_profile_dropdown: Rc<dyn Fn()> = Rc::new({
        let state = Rc::clone(&state);
        let profile_model = profile_model.clone();
        let profile_dropdown = profile_dropdown.clone();
        let populate_form = Rc::clone(&populate_form);
        move || {
            let state = state.borrow();
            let mut names = vec!["New connection".to_string()];
            names.extend(state.profiles.iter().map(|profile| profile.name.clone()));
            let name_refs = names.iter().map(String::as_str).collect::<Vec<_>>();
            profile_model.splice(0, profile_model.n_items(), &name_refs);

            let selected_index = state
                .last_used_profile
                .as_ref()
                .and_then(|name| {
                    state
                        .profiles
                        .iter()
                        .position(|profile| profile.name == *name)
                })
                .map(|index| index + 1)
                .unwrap_or(0);
            profile_dropdown.set_selected(selected_index as u32);
            populate_form(state.profiles.get(selected_index.saturating_sub(1)));
        }
    });

    {
        let auth_dropdown = auth_dropdown.clone();
        let update_auth_visibility = update_auth_visibility.clone();
        auth_dropdown.connect_selected_notify(move |dropdown| {
            update_auth_visibility(dropdown.selected() == 0);
        });
    }

    {
        let state = Rc::clone(&state);
        let populate_form = Rc::clone(&populate_form);
        profile_dropdown.connect_selected_notify(move |dropdown| {
            let index = dropdown.selected() as usize;
            let state = state.borrow();
            populate_form(state.profiles.get(index.saturating_sub(1)));
        });
    }

    let cancel_button = gtk::Button::with_label(&t!("common.cancel"));
    let save_button = gtk::Button::with_label(&t!("common.save"));
    let connect_button = gtk::Button::with_label("Connect");
    save_button.add_css_class("accent");
    connect_button.add_css_class("suggested-action");
    actions.append(&cancel_button);
    actions.append(&save_button);
    actions.append(&connect_button);
    window.set_default_widget(Some(&connect_button));

    {
        let window = window.clone();
        cancel_button.connect_clicked(move |_| {
            window.close();
        });
    }

    {
        let state = Rc::clone(&state);
        let callback = Rc::clone(&callback);
        let refresh_profile_dropdown = Rc::clone(&refresh_profile_dropdown);
        let profile_dropdown = profile_dropdown.clone();
        let profile_name_entry = profile_name_entry.clone();
        let host_entry = host_entry.clone();
        let port_input = port_input.clone();
        let username_entry = username_entry.clone();
        let start_directory_entry = start_directory_entry.clone();
        let skip_host_key_verification_switch = skip_host_key_verification_switch.clone();
        let auth_dropdown = auth_dropdown.clone();
        let private_key_entry = private_key_entry.clone();
        let public_key_entry = public_key_entry.clone();
        let save_profile_switch = save_profile_switch.clone();
        save_button.connect_clicked(move |_| {
            let current_index = profile_dropdown.selected() as usize;
            let previous_name = if current_index == 0 {
                None
            } else {
                state
                    .borrow()
                    .profiles
                    .get(current_index - 1)
                    .map(|profile| profile.name.clone())
            };
            let profile_name = profile_name_entry.text().trim().to_string();
            let host = host_entry.text().trim().to_string();
            let username = username_entry.text().trim().to_string();
            if profile_name.is_empty() || host.is_empty() || username.is_empty() {
                return;
            }

            let auth = if auth_dropdown.selected() == 0 {
                RemoteAuthConfig::Password {
                    username: username.clone(),
                }
            } else {
                let private_key = private_key_entry.text().trim().to_string();
                if private_key.is_empty() {
                    return;
                }
                RemoteAuthConfig::KeyFile {
                    username: username.clone(),
                    private_key_path: private_key.into(),
                    public_key_path: match public_key_entry.text().trim() {
                        "" => None,
                        value => Some(value.into()),
                    },
                }
            };

            let profile = RemoteProfile {
                name: profile_name,
                host,
                port: port_input.value() as u16,
                auth,
                start_directory: crate::remote::RemotePath::new(start_directory_entry.text()),
                skip_host_key_verification: skip_host_key_verification_switch.is_active(),
            };

            callback(RemoteDialogAction::SaveProfile {
                profile: profile.clone(),
                previous_name: previous_name.clone(),
            });

            {
                let mut state = state.borrow_mut();
                if let Some(previous_name) = previous_name {
                    if previous_name != profile.name {
                        state.profiles.retain(|item| item.name != previous_name);
                    }
                }
                if let Some(existing) = state
                    .profiles
                    .iter_mut()
                    .find(|item| item.name == profile.name)
                {
                    *existing = profile.clone();
                } else {
                    state.profiles.push(profile.clone());
                    state.profiles.sort_by(|left, right| {
                        left.name.to_lowercase().cmp(&right.name.to_lowercase())
                    });
                }
                state.last_used_profile = Some(profile.name.clone());
            }

            save_profile_switch.set_active(true);
            refresh_profile_dropdown();
        });
    }

    {
        let state = Rc::clone(&state);
        let callback = Rc::clone(&callback);
        let refresh_profile_dropdown = Rc::clone(&refresh_profile_dropdown);
        let profile_dropdown = profile_dropdown.clone();
        delete_profile_button.connect_clicked(move |_| {
            let index = profile_dropdown.selected() as usize;
            if index == 0 {
                return;
            }

            let name = {
                let state = state.borrow();
                state
                    .profiles
                    .get(index - 1)
                    .map(|profile| profile.name.clone())
            };
            let Some(name) = name else {
                return;
            };

            callback(RemoteDialogAction::DeleteProfile { name: name.clone() });

            {
                let mut state = state.borrow_mut();
                state.profiles.retain(|profile| profile.name != name);
                if state.last_used_profile.as_deref() == Some(name.as_str()) {
                    state.last_used_profile = None;
                }
            }

            refresh_profile_dropdown();
        });
    }

    {
        let window = window.clone();
        let callback = Rc::clone(&callback);
        let state = Rc::clone(&state);
        let profile_name_entry = profile_name_entry.clone();
        let host_entry = host_entry.clone();
        let port_input = port_input.clone();
        let username_entry = username_entry.clone();
        let start_directory_entry = start_directory_entry.clone();
        let skip_host_key_verification_switch = skip_host_key_verification_switch.clone();
        let auth_dropdown = auth_dropdown.clone();
        let password_entry = password_entry.clone();
        let private_key_entry = private_key_entry.clone();
        let public_key_entry = public_key_entry.clone();
        let save_profile_switch = save_profile_switch.clone();
        connect_button.connect_clicked(move |_| {
            let profile_name = profile_name_entry.text().to_string();
            let host = host_entry.text().trim().to_string();
            let username = username_entry.text().trim().to_string();
            if host.is_empty() || username.is_empty() {
                return;
            }

            let auth = if auth_dropdown.selected() == 0 {
                RemoteAuthConfig::Password {
                    username: username.clone(),
                }
            } else {
                let private_key = private_key_entry.text().trim().to_string();
                if private_key.is_empty() {
                    return;
                }
                RemoteAuthConfig::KeyFile {
                    username: username.clone(),
                    private_key_path: private_key.into(),
                    public_key_path: match public_key_entry.text().trim() {
                        "" => None,
                        value => Some(value.into()),
                    },
                }
            };

            let profile = RemoteProfile {
                name: if profile_name.trim().is_empty() {
                    format!("{username}@{host}")
                } else {
                    profile_name.trim().to_string()
                },
                host,
                port: port_input.value() as u16,
                auth,
                start_directory: crate::remote::RemotePath::new(start_directory_entry.text()),
                skip_host_key_verification: skip_host_key_verification_switch.is_active(),
            };
            let secret_text = password_entry.text().to_string();
            let secret = match &profile.auth {
                RemoteAuthConfig::Password { .. } => RemoteRuntimeSecret::Password(secret_text),
                RemoteAuthConfig::KeyFile { .. } if secret_text.is_empty() => {
                    RemoteRuntimeSecret::None
                }
                RemoteAuthConfig::KeyFile { .. } => RemoteRuntimeSecret::KeyPassphrase(secret_text),
            };
            let current_index = profile_dropdown.selected() as usize;
            let previous_name = if current_index == 0 {
                None
            } else {
                state
                    .borrow()
                    .profiles
                    .get(current_index - 1)
                    .map(|item| item.name.clone())
            };
            if save_profile_switch.is_active() {
                callback(RemoteDialogAction::SaveProfile {
                    profile: profile.clone(),
                    previous_name,
                });
            }
            callback(RemoteDialogAction::Connect(RemoteConnectionDialogResult {
                session: RemoteSession::new(profile.clone(), secret),
                last_used_profile: save_profile_switch.is_active().then_some(profile.name),
            }));
            window.close();
        });
    }

    refresh_profile_dropdown();

    glib::idle_add_local_once(move || {
        window.present();
        host_entry.grab_focus();
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
    let title = t!(
        "dialog.conflict_title",
        kind = presentation::file_operation_label(&conflict.kind)
    )
    .into_owned();

    let ModalWindow {
        window,
        content,
        actions,
    } = build_modal_window(parent, &title, 520, 240);

    let title_label = gtk::Label::new(Some(&title));
    title_label.set_xalign(0.0);
    title_label.set_wrap(true);
    title_label.add_css_class("dialog-title");
    content.append(&title_label);

    let detail_label = gtk::Label::new(Some(&detail));
    detail_label.set_xalign(0.0);
    detail_label.set_wrap(true);
    content.append(&detail_label);

    let cancel_button = gtk::Button::with_label(&t!("common.cancel"));
    let skip_button = gtk::Button::with_label(&t!("common.skip"));
    let rename_button = gtk::Button::with_label(&t!("common.rename"));
    let overwrite_button = gtk::Button::with_label(&t!("common.overwrite"));
    overwrite_button.add_css_class("suggested-action");
    actions.append(&cancel_button);
    actions.append(&skip_button);
    actions.append(&rename_button);
    actions.append(&overwrite_button);
    window.set_default_widget(Some(&skip_button));

    let callback = Rc::new(RefCell::new(Some(on_resolution)));
    let resolve = |resolution: ConflictResolution,
                   window: &gtk::Window,
                   callback: &Rc<RefCell<Option<F>>>| {
        if let Some(on_resolution) = callback.borrow_mut().take() {
            on_resolution(resolution);
        }
        window.close();
    };

    {
        let window = window.clone();
        let callback = Rc::clone(&callback);
        cancel_button.connect_clicked(move |_| {
            resolve(ConflictResolution::Cancel, &window, &callback);
        });
    }
    {
        let window = window.clone();
        let callback = Rc::clone(&callback);
        skip_button.connect_clicked(move |_| {
            resolve(ConflictResolution::Skip, &window, &callback);
        });
    }
    {
        let window = window.clone();
        let callback = Rc::clone(&callback);
        rename_button.connect_clicked(move |_| {
            resolve(ConflictResolution::Rename, &window, &callback);
        });
    }
    {
        let window = window.clone();
        let callback = Rc::clone(&callback);
        overwrite_button.connect_clicked(move |_| {
            resolve(ConflictResolution::Overwrite, &window, &callback);
        });
    }

    window.present();
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

    pub fn update_progress(&self, snapshot: &crate::application::OperationSnapshot) {
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

fn source_label(request: &OperationPlan) -> String {
    match request {
        OperationPlan::ArchiveExtract(request) => {
            if request.entry_paths.len() == 1 {
                request.entry_paths[0].clone()
            } else {
                t!("progress.selected_archive_items", count = request.entry_paths.len())
                    .into_owned()
            }
        }
        OperationPlan::RemoteDownload(request) => {
            if request.entry_paths.len() == 1 {
                request.entry_paths[0].clone()
            } else {
                t!("progress.selected_archive_items", count = request.entry_paths.len())
                    .into_owned()
            }
        }
        OperationPlan::Local(request) => selected_paths_summary(&request.sources),
        OperationPlan::RemoteUpload(request) => selected_paths_summary(&request.sources),
    }
}

fn target_label(request: &OperationPlan) -> String {
    match request {
        OperationPlan::ArchiveExtract(request) => request.target_directory.display().to_string(),
        OperationPlan::RemoteDownload(request) => request.target_directory.display().to_string(),
        OperationPlan::Local(request) => request
            .target_directory
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "-".into()),
        OperationPlan::RemoteUpload(request) => {
            let profile = request.session.profile();
            format!(
                "{}@{}:{}",
                profile.auth.username(),
                profile.host,
                request.target_directory
            )
        }
    }
}

fn selected_paths_summary(paths: &[PathBuf]) -> String {
    if paths.len() == 1 {
        return paths[0].display().to_string();
    }

    t!("common.items_count", count = paths.len()).into_owned()
}
