use std::rc::Rc;

use anyhow::Context;
use gtk::glib;
use rust_i18n::t;

use crate::ui::{
    dialogs, editor_dialog, file_viewer_dialog, file_viewer_image, main_window::MainWindow,
};

impl MainWindow {
    pub fn handle_properties(self: &Rc<Self>) {
        let (entry, location, calculated_dir_size) = {
            let commander = self.commander.borrow();
            let panel = commander.state().active_panel;
            let panel_state = commander.state().panel(panel);
            let Some(entry) = panel_state.selected_entry().cloned() else {
                dialogs::show_error(
                    &self.window,
                    &t!("error.properties_unavailable"),
                    &t!("error.no_entry_selected"),
                );
                return;
            };
            let calculated_dir_size = panel_state.calculated_dir_size(&entry.key());
            (entry, panel_state.location.clone(), calculated_dir_size)
        };

        if entry.is_parent_link {
            dialogs::show_error(
                &self.window,
                &t!("error.properties_unavailable"),
                &t!("error.parent_has_no_properties"),
            );
            return;
        }

        // Windows: native Explorer properties for local filesystem entries.
        #[cfg(target_os = "windows")]
        if let Some(path) = location.entry_filesystem_path(&entry) {
            if let Err(error) = crate::platform::show_file_properties(&path) {
                dialogs::show_error(
                    &self.window,
                    &t!("error.properties_unavailable"),
                    &error.to_string(),
                );
            }
            return;
        }

        // SFTP: editable Unix permissions (octal + checkboxes).
        if let crate::domain::PanelLocation::Remote(remote) = &location {
            let Some(remote_path) = entry.remote_path.clone() else {
                dialogs::show_error(
                    &self.window,
                    &t!("error.properties_unavailable"),
                    &t!("error.no_entry_selected"),
                );
                return;
            };
            let session_key = remote.session_key.clone();
            let reveal_name = entry.name.clone();
            let this = Rc::clone(self);
            dialogs::show_remote_properties(
                &self.window,
                &entry,
                &location,
                calculated_dir_size,
                move |mode| {
                    let session = match this.session_store.borrow().remote(&session_key) {
                        Some(session) => session,
                        None => {
                            dialogs::show_error(
                                &this.window,
                                &t!("error.properties_unavailable"),
                                &t!("error.remote_session_missing"),
                            );
                            return;
                        }
                    };
                    if let Err(error) =
                        this.remote_service
                            .set_permissions(&session, &remote_path, mode)
                    {
                        dialogs::show_error(
                            &this.window,
                            &t!("error.properties_unavailable"),
                            &error.to_string(),
                        );
                        return;
                    }
                    if let Some(parent) = crate::remote::RemotePath::new(&remote_path).parent() {
                        this.remote_service
                            .invalidate_listing_cache_for_path(parent.as_str());
                    }
                    this.refresh_active_panel_revealing(
                        &reveal_name,
                        t!(
                            "status.remote_permissions_updated",
                            path = remote_path.as_str(),
                            mode = format!("{mode:03o}")
                        )
                        .into_owned(),
                    );
                },
            );
            return;
        }

        dialogs::show_properties(&self.window, &entry, &location, calculated_dir_size);
    }

    pub fn handle_view(self: &Rc<Self>) {
        let selected = self
            .commander
            .borrow()
            .state()
            .active_panel()
            .selected_item();

        let Some(selected) = selected else {
            dialogs::show_error(
                &self.window,
                &t!("error.view_unavailable"),
                &t!("error.no_entry_selected"),
            );
            return;
        };

        if selected.is_parent_link {
            dialogs::show_error(
                &self.window,
                &t!("error.view_unavailable"),
                &t!("error.parent_cannot_be_viewed"),
            );
            return;
        }

        if selected.is_dir {
            dialogs::show_error(
                &self.window,
                &t!("error.view_unavailable"),
                &t!("error.directory_cannot_be_viewed"),
            );
            return;
        }

        if selected.archive_path.is_some() {
            dialogs::show_error(
                &self.window,
                &t!("error.view_unavailable"),
                &t!("error.archive_view_not_wired"),
            );
            return;
        }

        let Some(path) = selected.filesystem_path.clone() else {
            dialogs::show_error(
                &self.window,
                &t!("error.view_unavailable"),
                "Viewing remote files is not available yet.",
            );
            return;
        };

        let sibling_images = file_viewer_image::sibling_images_from_panel(
            self.commander.borrow().state().active_panel(),
        );
        let viewer_config = self.app_config_cache.borrow().viewer.clone();
        let theme = self.app_config_cache.borrow().general.theme;
        if let Err(error) = file_viewer_dialog::open(
            &self.window,
            path,
            viewer_config,
            theme,
            &self.archive_service.borrow(),
            sibling_images,
        ) {
            dialogs::show_error(
                &self.window,
                &t!("error.could_not_open_viewer"),
                &error.to_string(),
            );
        }
    }

    pub fn handle_rename(self: &Rc<Self>) {
        let (panel, selected, cursor) = {
            let commander = self.commander.borrow();
            let panel = commander.state().active_panel;
            let panel_state = commander.state().panel(panel);
            let selected = panel_state.selected_item();
            let cursor = panel_state.cursor_index();
            (panel, selected, cursor)
        };

        let Some(selected) = selected else {
            dialogs::show_error(
                &self.window,
                &t!("error.rename_unavailable"),
                &t!("error.no_entry_selected"),
            );
            return;
        };

        if selected.is_parent_link {
            dialogs::show_error(
                &self.window,
                &t!("error.rename_unavailable"),
                &t!("error.parent_cannot_be_renamed"),
            );
            return;
        }

        if selected.archive_path.is_some() {
            dialogs::show_error(
                &self.window,
                &t!("error.rename_unavailable"),
                &t!("error.archive_rename_not_supported"),
            );
            return;
        }

        let Some(index) = cursor else {
            dialogs::show_error(
                &self.window,
                &t!("error.rename_unavailable"),
                &t!("error.no_entry_selected"),
            );
            return;
        };

        let this = Rc::clone(self);
        let display_name = selected.display_name.clone();
        let is_dir = selected.is_dir;
        self.view
            .panel(panel)
            .begin_inline_rename(index, &display_name, is_dir, {
                let display_name = display_name.clone();
                move |new_name| {
                    let new_name = new_name.trim().to_string();
                    if new_name.is_empty() || new_name == display_name {
                        return;
                    }

                    let renamed_path = selected
                        .filesystem_path
                        .as_ref()
                        .map(|path| path.with_file_name(&new_name).display().to_string())
                        .unwrap_or_else(|| new_name.clone());

                    let (source, target) =
                        match this.commander.borrow().rename_active_request(&new_name) {
                            Ok(paths) => paths,
                            Err(error) => {
                                this.show_command_failed(error);
                                return;
                            }
                        };

                    if let Err(error) = crate::fs::reader::rename_path(&source, &target) {
                        this.show_command_failed(error);
                        return;
                    }

                    let status = t!("status.renamed", path = renamed_path.as_str()).into_owned();
                    // Keep model in sync, but refresh the panel on the next idle — immediate
                    // set_entries after tearing down the rename Entry aborts on Windows.
                    let _ = this
                        .commander
                        .borrow_mut()
                        .apply_active_rename(&new_name, status.clone());
                    let this = Rc::clone(&this);
                    glib::idle_add_local_once(move || {
                        this.refresh_active_panel_revealing(&new_name, status);
                    });
                }
            });
    }

    pub fn handle_open_console(self: &Rc<Self>) {
        let path = self
            .commander
            .borrow()
            .state()
            .active_panel()
            .location
            .host_directory();

        let Some(path) = path else {
            dialogs::show_error(
                &self.window,
                &t!("error.could_not_open_console"),
                "A local terminal can only be opened from filesystem or archive views.",
            );
            return;
        };

        if let Err(error) = self.platform_port.open_console(&path) {
            dialogs::show_error(
                &self.window,
                &t!("error.could_not_open_console"),
                &error.to_string(),
            );
            return;
        }

        let update = {
            let mut commander = self.commander.borrow_mut();
            commander.set_status(
                t!(
                    "status.console_opened_at",
                    path = path.display().to_string()
                )
                .into_owned(),
            )
        };
        self.apply_update(update);
    }

    pub fn handle_edit(self: &Rc<Self>) {
        let (selected, size_bytes, location) = {
            let commander = self.commander.borrow();
            let panel = commander.state().active_panel;
            let panel_state = commander.state().panel(panel);
            let size_bytes = panel_state
                .selected_entry()
                .map(|entry| entry.size_bytes)
                .unwrap_or(0);
            (
                panel_state.selected_item(),
                size_bytes,
                panel_state.location.clone(),
            )
        };

        let Some(selected) = selected else {
            dialogs::show_error(
                &self.window,
                &t!("error.edit_unavailable"),
                &t!("error.no_entry_selected"),
            );
            return;
        };

        if selected.is_parent_link {
            dialogs::show_error(
                &self.window,
                &t!("error.edit_unavailable"),
                &t!("error.parent_cannot_be_edited"),
            );
            return;
        }

        if selected.is_dir {
            dialogs::show_error(
                &self.window,
                &t!("error.edit_unavailable"),
                &t!("error.directory_cannot_be_edited"),
            );
            return;
        }

        if selected.archive_path.is_some() {
            dialogs::show_error(
                &self.window,
                &t!("error.edit_unavailable"),
                &t!("error.archive_edit_not_supported"),
            );
            return;
        }

        let max_bytes = self.app_config_cache.borrow().editor.max_edit_size_bytes();
        if size_bytes > max_bytes {
            dialogs::show_error(
                &self.window,
                &t!("error.edit_unavailable"),
                &t!(
                    "error.edit_file_too_large",
                    size = crate::fs::reader::format_bytes(size_bytes),
                    limit = crate::fs::reader::format_bytes(max_bytes)
                ),
            );
            return;
        }

        if let Some(path) = selected.filesystem_path.clone() {
            let theme_preference = self.app_config_cache.borrow().general.theme;
            let this = Rc::clone(self);
            if let Err(error) = editor_dialog::edit_file(
                &self.window,
                path,
                theme_preference,
                self.config_store.clone(),
                move |path| {
                    this.set_status_message(
                        t!("status.saved", path = path.display().to_string()).into_owned(),
                    );
                },
            ) {
                dialogs::show_error(
                    &self.window,
                    &t!("error.could_not_open_editor"),
                    &error.to_string(),
                );
            }
            return;
        }

        let Some(remote_path) = selected.remote_path.clone() else {
            dialogs::show_error(
                &self.window,
                &t!("error.edit_unavailable"),
                &t!("error.edit_filesystem_or_remote_only"),
            );
            return;
        };

        let crate::domain::PanelLocation::Remote(remote_location) = location else {
            dialogs::show_error(
                &self.window,
                &t!("error.edit_unavailable"),
                &t!("error.edit_filesystem_or_remote_only"),
            );
            return;
        };

        self.start_remote_edit(remote_location.session_key, remote_path);
    }

    fn start_remote_edit(self: &Rc<Self>, session_key: String, remote_path: String) {
        let session = match self.session_store.borrow().remote(&session_key) {
            Some(session) => session,
            None => {
                dialogs::show_error(
                    &self.window,
                    &t!("error.edit_unavailable"),
                    &t!("error.remote_session_missing"),
                );
                return;
            }
        };

        let remote = crate::remote::RemotePath::new(&remote_path);
        let file_name = remote.file_name().unwrap_or("remote-file.txt");
        let (temp_dir, local_path) = match crate::fs::temp::create_remote_edit_temp_dir(file_name) {
            Ok(paths) => paths,
            Err(error) => {
                dialogs::show_error(
                    &self.window,
                    &t!("error.could_not_open_editor"),
                    &error.to_string(),
                );
                return;
            }
        };

        let remote_parent = crate::remote::RemotePath::new(&remote_path)
            .parent()
            .map(|path| path.to_string())
            .unwrap_or_else(|| "/".into());

        let download = crate::application::OperationPlan::RemoteDownload(
            crate::application::RemoteDownloadRequest {
                session: session.clone(),
                entry_paths: vec![remote_path.clone()],
                target_directory: temp_dir.clone(),
                force_overwrite: true,
            },
        );

        let this = Rc::clone(self);
        let theme_preference = self.app_config_cache.borrow().general.theme;
        let config_store = self.config_store.clone();
        let window = self.window.clone();
        let remote_path_for_status = remote_path.clone();

        self.operations_controller()
            .run_remote_transfer_with_progress(
                download,
                &t!("progress.remote_edit_download"),
                true,
                move |result| {
                    if let Err(error) = result {
                        crate::fs::temp::remove_temp_dir(&temp_dir);
                        dialogs::show_error(&window, &t!("error.could_not_open_editor"), &error);
                        return;
                    }

                    if !local_path.is_file() {
                        crate::fs::temp::remove_temp_dir(&temp_dir);
                        dialogs::show_error(
                            &window,
                            &t!("error.could_not_open_editor"),
                            &t!("error.remote_edit_download_missing"),
                        );
                        return;
                    }

                    let binding = editor_dialog::RemoteEditBinding {
                        session: session.clone(),
                        remote_path: remote_path.clone(),
                        temp_dir: temp_dir.clone(),
                    };
                    let remote_parent = remote_parent.clone();
                    let upload_hook: editor_dialog::RemoteUploadHook = {
                        let this = Rc::clone(&this);
                        Rc::new(move |local_path, binding, done| {
                            let upload = crate::application::OperationPlan::RemoteUpload(
                                crate::application::RemoteUploadRequest {
                                    sources: vec![local_path],
                                    session: binding.session.clone(),
                                    target_directory: remote_parent.clone(),
                                    force_overwrite: true,
                                },
                            );
                            this.operations_controller()
                                .run_remote_transfer_with_progress(
                                    upload,
                                    &t!("progress.remote_edit_upload"),
                                    true,
                                    done,
                                );
                        })
                    };

                    let this = Rc::clone(&this);
                    let remote_path_for_status = remote_path_for_status.clone();
                    if let Err(error) = editor_dialog::edit_remote_file(
                        &window,
                        local_path,
                        binding,
                        upload_hook,
                        theme_preference,
                        config_store.clone(),
                        move |_| {
                            this.set_status_message(
                                t!(
                                    "status.remote_saved",
                                    path = remote_path_for_status.as_str()
                                )
                                .into_owned(),
                            );
                        },
                    ) {
                        crate::fs::temp::remove_temp_dir(&temp_dir);
                        dialogs::show_error(
                            &window,
                            &t!("error.could_not_open_editor"),
                            &error.to_string(),
                        );
                    }
                },
            );
    }

    pub fn handle_make_directory(self: &Rc<Self>) {
        let this = Rc::clone(self);
        dialogs::prompt_new_directory(&self.window, move |name| {
            let target = {
                let commander = this.commander.borrow();
                commander.create_directory_request(&name)
            };
            let target = match target {
                Ok(target) => target,
                Err(error) => {
                    this.show_command_failed(error);
                    return;
                }
            };

            let display_path = match &target {
                crate::application::CreateDirectoryTarget::Filesystem(path) => {
                    if let Err(error) = std::fs::create_dir(path)
                        .with_context(|| format!("Could not create directory {}", path.display()))
                    {
                        this.show_command_failed(error);
                        return;
                    }
                    path.display().to_string()
                }
                crate::application::CreateDirectoryTarget::Remote { session_key, path } => {
                    let session = match this.session_store.borrow().remote(session_key) {
                        Some(session) => session,
                        None => {
                            this.show_command_failed(t!("error.remote_session_missing"));
                            return;
                        }
                    };
                    if let Err(error) = this.remote_service.create_directory(&session, path) {
                        this.show_command_failed(error);
                        return;
                    }
                    path.clone()
                }
            };

            let name = name.trim().to_string();
            this.refresh_active_panel_revealing(
                &name,
                t!("status.created_directory", path = display_path).into_owned(),
            );
        });
    }

    pub fn handle_create_text_file(self: &Rc<Self>) {
        let this = Rc::clone(self);
        dialogs::prompt_new_file(&self.window, move |name| {
            let trimmed = name.trim().to_string();
            let target = {
                let commander = this.commander.borrow();
                commander.create_file_request(&trimmed)
            };
            let target = match target {
                Ok(target) => target,
                Err(error) => {
                    this.show_command_failed(error);
                    return;
                }
            };

            if let Err(error) = std::fs::write(&target, "")
                .with_context(|| format!("Could not create file {}", target.display()))
            {
                this.show_command_failed(error);
                return;
            }

            this.suppress_panel_focus.set(true);
            this.refresh_active_panel_revealing(&trimmed, {
                t!("status.created_file", path = target.display().to_string()).into_owned()
            });

            let theme_preference = this.app_config_cache.borrow().general.theme;
            let editor_this = Rc::clone(&this);
            if let Err(error) = editor_dialog::edit_file(
                &this.window,
                target,
                theme_preference,
                this.config_store.clone(),
                move |path| {
                    editor_this.set_status_message(
                        t!("status.saved", path = path.display().to_string()).into_owned(),
                    );
                },
            ) {
                this.suppress_panel_focus.set(false);
                this.show_command_failed(error);
                return;
            }

            this.clear_panel_focus_suppress_after(std::time::Duration::from_millis(500));
        });
    }

    pub fn handle_copy_with_rename(self: &Rc<Self>) {
        let selected = self
            .commander
            .borrow()
            .state()
            .active_panel()
            .selected_item();

        let Some(selected) = selected else {
            dialogs::show_error(
                &self.window,
                &t!("error.copy_unavailable"),
                &t!("error.no_entry_selected"),
            );
            return;
        };

        if selected.is_parent_link {
            dialogs::show_error(
                &self.window,
                &t!("error.copy_unavailable"),
                &t!("error.parent_cannot_be_copied"),
            );
            return;
        }

        if selected.filesystem_path.is_none() {
            dialogs::show_error(
                &self.window,
                &t!("error.copy_unavailable"),
                &t!("error.copy_rename_filesystem_only"),
            );
            return;
        }

        let this = Rc::clone(self);
        dialogs::prompt_copy_as(&self.window, selected.display_name, move |new_name| {
            let trimmed = new_name.trim().to_string();
            let (source, target) = {
                let commander = this.commander.borrow();
                match commander.copy_with_rename_request(&trimmed) {
                    Ok(paths) => paths,
                    Err(error) => {
                        this.show_command_failed(error);
                        return;
                    }
                }
            };

            if let Err(error) = crate::fs::operations::copy_entry(&source, &target) {
                this.show_command_failed(error);
                return;
            }

            this.refresh_active_panel_revealing(
                &trimmed,
                t!(
                    "status.copied_as",
                    source = source.display().to_string(),
                    target = target.display().to_string()
                )
                .into_owned(),
            );
        });
    }
}
