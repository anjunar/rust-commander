use std::rc::Rc;

use rust_i18n::t;

use crate::{
    archive::ArchiveService,
    remote::RemoteProfile,
    ui::{dialogs, main_window::MainWindow},
};

impl MainWindow {
    pub fn handle_help(self: &Rc<Self>) {
        let current_config = self.config_store.snapshot();
        let this = Rc::clone(self);
        dialogs::show_settings(&self.window, current_config, move |next_config| {
            if let Err(error) = this.config_store.save(next_config.clone()) {
                dialogs::show_error(
                    &this.window,
                    &t!("error.could_not_save_settings"),
                    &error.to_string(),
                );
                return;
            }

            this.archive_service
                .replace(ArchiveService::with_default_backends(
                    this.task_spawner.clone(),
                ));
            let selected_locale = crate::i18n::apply_locale(next_config.locale.language.as_deref());

            let update = {
                let mut commander = this.commander.borrow_mut();
                commander.apply_archive_config(next_config.archive.clone());
                let mut update = match commander.apply_panel_settings(next_config.panels.clone()) {
                    Ok(update) => update,
                    Err(error) => {
                        dialogs::show_error(
                            &this.window,
                            &t!("error.could_not_save_settings"),
                            &error.to_string(),
                        );
                        return;
                    }
                };
                commander.set_status(
                    t!(
                        "status.language_changed",
                        language = crate::i18n::locale_display_name(selected_locale)
                    )
                    .into_owned(),
                );
                update.status = true;
                update
            };
            this.apply_update(update);
            this.window_chrome().apply_theme();
            this.window_chrome()
                .refresh_localized_labels(&this.commander_view, &this.terminal_dock);
        });
    }

    pub fn handle_connect_remote(self: &Rc<Self>) {
        let active_panel = self.commander.borrow().state().active_panel;
        let remote_config = self.app_config_cache.borrow().remote.clone();
        let this = Rc::clone(self);
        dialogs::prompt_remote_connection(
            &self.window,
            remote_config,
            move |action| match action {
                dialogs::RemoteDialogAction::Connect(result) => {
                    if let Some(profile_name) = result.last_used_profile {
                        if let Err(error) = this.update_remote_config(|remote| {
                            remote.last_used_profile = Some(profile_name)
                        }) {
                            dialogs::show_error(
                                &this.window,
                                &t!("error.could_not_save_settings"),
                                &error.to_string(),
                            );
                            return;
                        }
                    }

                    this.navigation_controller()
                        .start_remote_session(active_panel, result.session);
                }
                dialogs::RemoteDialogAction::SaveProfile {
                    profile,
                    previous_name,
                } => {
                    if let Err(error) = this.update_remote_config(|remote| {
                        if let Some(previous_name) = previous_name.as_ref() {
                            if previous_name != &profile.name {
                                remote.profiles.retain(|item| item.name != *previous_name);
                            }
                        }
                        upsert_remote_profile(&mut remote.profiles, profile.clone());
                        remote.last_used_profile = Some(profile.name.clone());
                    }) {
                        dialogs::show_error(
                            &this.window,
                            &t!("error.could_not_save_settings"),
                            &error.to_string(),
                        );
                    }
                }
                dialogs::RemoteDialogAction::DeleteProfile { name } => {
                    if let Err(error) = this.update_remote_config(|remote| {
                        remote.profiles.retain(|profile| profile.name != name);
                        if remote.last_used_profile.as_deref() == Some(name.as_str()) {
                            remote.last_used_profile = None;
                        }
                    }) {
                        dialogs::show_error(
                            &this.window,
                            &t!("error.could_not_save_settings"),
                            &error.to_string(),
                        );
                    }
                }
            },
        );
    }

    fn update_remote_config(
        &self,
        update_remote: impl FnOnce(&mut crate::remote::RemoteConfig),
    ) -> anyhow::Result<()> {
        self.config_store.update(|next_config| {
            update_remote(&mut next_config.remote);
        })?;
        Ok(())
    }
}

fn upsert_remote_profile(profiles: &mut Vec<RemoteProfile>, profile: RemoteProfile) {
    if let Some(existing) = profiles.iter_mut().find(|item| item.name == profile.name) {
        *existing = profile;
    } else {
        profiles.push(profile);
        profiles.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    }
}
