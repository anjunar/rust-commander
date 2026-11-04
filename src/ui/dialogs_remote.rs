use std::{cell::RefCell, rc::Rc};

use gtk::prelude::*;

use crate::{
    remote::{
        RemoteAuthConfig, RemoteConfig, RemotePath, RemoteProfile, RemoteRuntimeSecret,
        RemoteSession,
    },
    ui::dsl::{button, dropdown, dropdown_list, password, spin, ListProperty, Property},
};

use super::dialogs_base::{field, hint, modal, present_and_focus, row, switch_row};

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

type ActionCallback = Rc<dyn Fn(RemoteDialogAction)>;
type Refresh = Rc<dyn Fn()>;
type SharedConfig = Rc<RefCell<RemoteConfig>>;

const AUTH_PASSWORD: u32 = 0;
const AUTH_KEY_FILE: u32 = 1;

pub fn prompt_remote_connection<F>(
    parent: &gtk::ApplicationWindow,
    remote_config: RemoteConfig,
    on_action: F,
) where
    F: Fn(RemoteDialogAction) + 'static,
{
    let state: SharedConfig = Rc::new(RefCell::new(remote_config));
    let form = Rc::new(RemoteForm::new());
    let callback: ActionCallback = Rc::new(on_action);

    // Rebuilds the profile picker from the config and selects the last used profile.
    let refresh: Refresh = Rc::new({
        let state = Rc::clone(&state);
        let form = Rc::clone(&form);
        move || {
            let (names, selected) = {
                let state = state.borrow();
                let mut names = vec!["New connection".to_string()];
                names.extend(state.profiles.iter().map(|profile| profile.name.clone()));
                let selected = state
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
                (names, selected as u32)
            };
            form.profile_names.replace_all(names);
            form.selected_profile.set_always(selected);
        }
    });

    let uses_key_file = form.auth_method.map(|method| *method == AUTH_KEY_FILE);
    let has_saved_profile = form.selected_profile.map(|index| *index != 0);

    let (window, (host_entry, connect_button)) = modal(
        parent,
        "Connect To Remote",
        (520, 470),
        |dialog| {
            // Picking a saved profile loads it into the form; "New connection" clears it.
            {
                let state = Rc::clone(&state);
                let form = Rc::clone(&form);
                let selected_profile = form.selected_profile.clone();
                dialog
                    .window()
                    .add_disposable(selected_profile.observe(move |index| {
                        let profile = saved_profile(&state.borrow(), *index);
                        form.populate(profile.as_ref());
                    }));
            }

            let host_entry = dialog.content(|| {
                row("Saved profile", || {
                    dropdown_list(&form.profile_names, &form.selected_profile, |_| {});
                });
                field("Profile name", &form.profile_name);
                let host_entry = field("Host", &form.host);
                row("Port", || {
                    spin(&form.port, 1.0, 65535.0, 1.0, |_| {});
                });
                field("Username", &form.username);
                field("Start directory", &form.start_directory);
                switch_row(
                    "Insecure: skip host key verification",
                    &form.skip_host_key_verification,
                );
                hint("Only use this for test or demo servers. This disables known_hosts validation and allows man-in-the-middle attacks.");

                row("Authentication", || {
                    dropdown(&["Password", "Key file"], &form.auth_method, |_| {});
                });
                password(&form.secret, |entry| {
                    entry.set_placeholder_text(Some("Password or passphrase"));
                });
                let private_key = field("Private key path", &form.private_key_path);
                private_key.bind_visible(&uses_key_file);
                private_key.bind_sensitive(&uses_key_file);
                let public_key = field("Public key path (optional)", &form.public_key_path);
                public_key.bind_visible(&uses_key_file);
                public_key.bind_sensitive(&uses_key_file);

                switch_row("Save profile without secret", &form.save_profile);
                button("Delete profile", |button| {
                    button.css("destructive-action");
                    button.bind_sensitive(&has_saved_profile);
                    let state = Rc::clone(&state);
                    let form = Rc::clone(&form);
                    let callback = Rc::clone(&callback);
                    let refresh = Rc::clone(&refresh);
                    button.on_click(move || delete_profile(&state, &form, &callback, &refresh));
                });

                host_entry
            });

            let connect_button = dialog.actions(|| {
                dialog.close_button("Cancel");
                button("Save", |button| {
                    button.css("accent");
                    let state = Rc::clone(&state);
                    let form = Rc::clone(&form);
                    let callback = Rc::clone(&callback);
                    let refresh = Rc::clone(&refresh);
                    button.on_click(move || save_profile(&state, &form, &callback, &refresh));
                });
                button("Connect", |button| {
                    button.css("suggested-action");
                    let state = Rc::clone(&state);
                    let form = Rc::clone(&form);
                    let callback = Rc::clone(&callback);
                    let window = dialog.window().widget().clone();
                    button.on_click(move || connect(&state, &form, &callback, &window));
                })
            });

            (host_entry, connect_button)
        },
    );
    window.set_default_widget(Some(connect_button.widget()));

    refresh();
    present_and_focus(&window, host_entry.widget());
}

/// The editable state of the connection form.
struct RemoteForm {
    profile_names: ListProperty<String>,
    selected_profile: Property<u32>,
    profile_name: Property<String>,
    host: Property<String>,
    port: Property<f64>,
    username: Property<String>,
    start_directory: Property<String>,
    skip_host_key_verification: Property<bool>,
    auth_method: Property<u32>,
    secret: Property<String>,
    private_key_path: Property<String>,
    public_key_path: Property<String>,
    save_profile: Property<bool>,
}

impl RemoteForm {
    fn new() -> Self {
        Self {
            profile_names: ListProperty::default(),
            selected_profile: Property::new(0),
            profile_name: Property::new(String::new()),
            host: Property::new(String::new()),
            port: Property::new(22.0),
            username: Property::new(String::new()),
            start_directory: Property::new("/".to_string()),
            skip_host_key_verification: Property::new(false),
            auth_method: Property::new(AUTH_PASSWORD),
            secret: Property::new(String::new()),
            private_key_path: Property::new(String::new()),
            public_key_path: Property::new(String::new()),
            save_profile: Property::new(false),
        }
    }

    /// Loads a saved profile into the form, or clears it for a new connection.
    fn populate(&self, profile: Option<&RemoteProfile>) {
        let Some(profile) = profile else {
            self.clear();
            return;
        };

        self.profile_name.set(profile.name.clone());
        self.host.set(profile.host.clone());
        self.port.set(profile.port as f64);
        self.username.set(profile.auth.username().to_string());
        self.start_directory
            .set(profile.start_directory.as_str().to_string());
        self.skip_host_key_verification
            .set(profile.skip_host_key_verification);
        match &profile.auth {
            RemoteAuthConfig::Password { .. } => {
                self.auth_method.set(AUTH_PASSWORD);
                self.private_key_path.set(String::new());
                self.public_key_path.set(String::new());
            }
            RemoteAuthConfig::KeyFile {
                private_key_path,
                public_key_path,
                ..
            } => {
                self.auth_method.set(AUTH_KEY_FILE);
                self.private_key_path
                    .set(private_key_path.display().to_string());
                self.public_key_path.set(
                    public_key_path
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_default(),
                );
            }
        }
        self.secret.set(String::new());
        self.save_profile.set(true);
    }

    fn clear(&self) {
        self.profile_name.set(String::new());
        self.host.set(String::new());
        self.port.set(22.0);
        self.username.set(String::new());
        self.start_directory.set("/".to_string());
        self.skip_host_key_verification.set(false);
        self.auth_method.set(AUTH_PASSWORD);
        self.secret.set(String::new());
        self.private_key_path.set(String::new());
        self.public_key_path.set(String::new());
        self.save_profile.set(false);
    }

    /// The profile described by the form, or `None` while a required field is empty.
    fn profile(&self, name: String) -> Option<RemoteProfile> {
        let host = self.host.get().trim().to_string();
        let username = self.username.get().trim().to_string();
        if host.is_empty() || username.is_empty() {
            return None;
        }

        let auth = if self.auth_method.get() == AUTH_PASSWORD {
            RemoteAuthConfig::Password { username }
        } else {
            let private_key = self.private_key_path.get().trim().to_string();
            if private_key.is_empty() {
                return None;
            }
            let public_key = self.public_key_path.get();
            RemoteAuthConfig::KeyFile {
                username,
                private_key_path: private_key.into(),
                public_key_path: match public_key.trim() {
                    "" => None,
                    value => Some(value.into()),
                },
            }
        };

        Some(RemoteProfile {
            name,
            host,
            port: self.port.get() as u16,
            auth,
            start_directory: RemotePath::new(self.start_directory.get()),
            skip_host_key_verification: self.skip_host_key_verification.get(),
        })
    }
}

/// The saved profile behind a picker index; index 0 is "New connection".
fn saved_profile(config: &RemoteConfig, index: u32) -> Option<RemoteProfile> {
    (index as usize)
        .checked_sub(1)
        .and_then(|position| config.profiles.get(position).cloned())
}

fn save_profile(
    state: &SharedConfig,
    form: &RemoteForm,
    callback: &ActionCallback,
    refresh: &Refresh,
) {
    let previous_name =
        saved_profile(&state.borrow(), form.selected_profile.get()).map(|profile| profile.name);
    let name = form.profile_name.get().trim().to_string();
    if name.is_empty() {
        return;
    }
    let Some(profile) = form.profile(name) else {
        return;
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
            state
                .profiles
                .sort_by_key(|profile| profile.name.to_lowercase());
        }
        state.last_used_profile = Some(profile.name.clone());
    }

    form.save_profile.set(true);
    refresh();
}

fn delete_profile(
    state: &SharedConfig,
    form: &RemoteForm,
    callback: &ActionCallback,
    refresh: &Refresh,
) {
    let name =
        saved_profile(&state.borrow(), form.selected_profile.get()).map(|profile| profile.name);
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

    refresh();
}

fn connect(
    state: &SharedConfig,
    form: &RemoteForm,
    callback: &ActionCallback,
    window: &gtk::Window,
) {
    let Some(mut profile) = form.profile(String::new()) else {
        return;
    };
    let typed_name = form.profile_name.get().trim().to_string();
    profile.name = if typed_name.is_empty() {
        format!("{}@{}", profile.auth.username(), profile.host)
    } else {
        typed_name
    };

    let secret_text = form.secret.get();
    let secret = match &profile.auth {
        RemoteAuthConfig::Password { .. } => RemoteRuntimeSecret::Password(secret_text),
        RemoteAuthConfig::KeyFile { .. } if secret_text.is_empty() => RemoteRuntimeSecret::None,
        RemoteAuthConfig::KeyFile { .. } => RemoteRuntimeSecret::KeyPassphrase(secret_text),
    };

    let previous_name =
        saved_profile(&state.borrow(), form.selected_profile.get()).map(|profile| profile.name);
    let save = form.save_profile.get();
    if save {
        callback(RemoteDialogAction::SaveProfile {
            profile: profile.clone(),
            previous_name,
        });
    }
    callback(RemoteDialogAction::Connect(RemoteConnectionDialogResult {
        session: RemoteSession::new(profile.clone(), secret),
        last_used_profile: save.then_some(profile.name),
    }));
    window.close();
}
