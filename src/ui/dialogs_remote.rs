use std::{cell::RefCell, rc::Rc};

use gtk::{glib, prelude::*};

use crate::remote::{
    RemoteAuthConfig, RemoteConfig, RemoteProfile, RemoteRuntimeSecret, RemoteSession,
};

use super::build_modal_window;
use super::dialogs_base::ModalWindow;

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

    let cancel_button = gtk::Button::with_label("Cancel");
    let save_button = gtk::Button::with_label("Save");
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

fn settings_row() -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Horizontal, 8)
}

fn row_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label
}

fn switch_row(text: &str, switch: &gtk::Switch) -> gtk::Box {
    let row = settings_row();
    row.append(&row_label(text));
    row.append(switch);
    row
}
