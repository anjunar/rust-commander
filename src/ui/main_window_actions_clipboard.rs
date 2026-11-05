use std::path::PathBuf;
use std::rc::Rc;

use rust_i18n::t;

use crate::{
    application::{FileOperationKind, LocalOperationRequest, OperationPlan, RemoteUploadRequest},
    domain::PanelLocation,
    ui::{
        dialogs,
        file_clipboard::{
            clear_system_file_clipboard, existing_paths, read_system_file_clipboard,
            set_system_file_clipboard, FileClipboardMode,
        },
        main_window::MainWindow,
    },
};

impl MainWindow {
    pub fn handle_clipboard_copy(self: &Rc<Self>) {
        self.stage_clipboard_selection(FileClipboardMode::Copy);
    }

    pub fn handle_clipboard_cut(self: &Rc<Self>) {
        self.stage_clipboard_selection(FileClipboardMode::Cut);
    }

    pub fn handle_clipboard_paste(self: &Rc<Self>) {
        if self.view.terminal.has_focus() {
            return;
        }

        #[cfg(target_os = "windows")]
        {
            self.resolve_and_paste(read_system_file_clipboard());
        }

        #[cfg(not(target_os = "windows"))]
        {
            let this = Rc::clone(self);
            crate::ui::file_clipboard::read_system_file_clipboard_async(move |system| {
                this.resolve_and_paste(system);
            });
        }
    }

    fn resolve_and_paste(self: &Rc<Self>, system: Option<(FileClipboardMode, Vec<PathBuf>)>) {
        let internal = {
            let clipboard = self.file_clipboard.borrow();
            if clipboard.is_empty() {
                None
            } else {
                Some((clipboard.mode(), clipboard.paths().to_vec()))
            }
        };

        // Prefer the OS clipboard so Explorer / other managers win over a stale
        // in-app stage when the user copied elsewhere.
        let (mode, paths) = match system {
            Some((mode, paths)) => (mode, paths),
            None => match internal {
                Some((mode, paths)) => (mode, existing_paths(&paths)),
                None => {
                    dialogs::show_error(
                        &self.window,
                        &t!("error.clipboard_unavailable"),
                        &t!("error.clipboard_empty"),
                    );
                    return;
                }
            },
        };

        if paths.is_empty() {
            self.file_clipboard.borrow_mut().clear();
            dialogs::show_error(
                &self.window,
                &t!("error.clipboard_unavailable"),
                &t!("error.clipboard_sources_missing"),
            );
            return;
        }

        self.paste_clipboard_paths(mode, paths);
    }

    fn stage_clipboard_selection(self: &Rc<Self>, mode: FileClipboardMode) {
        if self.view.terminal.has_focus() {
            return;
        }

        let paths = {
            let commander = self.commander.borrow();
            let panel = commander.state().active_panel();
            if panel.location.filesystem_path().is_none() {
                dialogs::show_error(
                    &self.window,
                    &t!("error.clipboard_unavailable"),
                    &t!("error.clipboard_filesystem_only"),
                );
                return;
            }

            panel
                .selected_items()
                .into_iter()
                .filter_map(|item| item.filesystem_path)
                .collect::<Vec<_>>()
        };

        if paths.is_empty() {
            dialogs::show_error(
                &self.window,
                &t!("error.clipboard_unavailable"),
                &t!("error.no_entry_selected"),
            );
            return;
        }

        let count = paths.len();
        self.file_clipboard.borrow_mut().set(mode, paths.clone());
        if !set_system_file_clipboard(&paths, mode) {
            dialogs::show_error(
                &self.window,
                &t!("error.clipboard_unavailable"),
                &t!("error.clipboard_set_failed"),
            );
            return;
        }

        let status = match mode {
            FileClipboardMode::Copy => t!("status.clipboard_copied", count = count).into_owned(),
            FileClipboardMode::Cut => t!("status.clipboard_cut", count = count).into_owned(),
        };
        self.set_status_message(status);
    }

    fn paste_clipboard_paths(self: &Rc<Self>, mode: FileClipboardMode, sources: Vec<PathBuf>) {
        let plan = {
            let commander = self.commander.borrow();
            let target_panel = commander.state().active_panel();
            match (&target_panel.location, mode) {
                (PanelLocation::Filesystem(_), _) => {
                    let Some(target_directory) = target_panel.location.host_directory() else {
                        dialogs::show_error(
                            &self.window,
                            &t!("error.operation_unavailable"),
                            &t!("error.no_filesystem_target_directory"),
                        );
                        return;
                    };
                    let kind = match mode {
                        FileClipboardMode::Copy => FileOperationKind::Copy,
                        FileClipboardMode::Cut => FileOperationKind::Move,
                    };
                    OperationPlan::Local(LocalOperationRequest {
                        kind,
                        sources,
                        target_directory: Some(target_directory),
                        target_path: None,
                        use_recycle_bin: false,
                    })
                }
                (PanelLocation::Remote(location), FileClipboardMode::Copy) => {
                    let session = match self.session_store.borrow().remote(&location.session_key) {
                        Some(session) => session,
                        None => {
                            dialogs::show_error(
                                &self.window,
                                &t!("error.operation_unavailable"),
                                &t!("error.remote_session_missing"),
                            );
                            return;
                        }
                    };
                    OperationPlan::RemoteUpload(RemoteUploadRequest {
                        sources,
                        session,
                        target_directory: location.current_path.clone(),
                        force_overwrite: false,
                    })
                }
                (PanelLocation::Remote(_), FileClipboardMode::Cut) => {
                    dialogs::show_error(
                        &self.window,
                        &t!("error.operation_unavailable"),
                        &t!("error.clipboard_cut_remote_unsupported"),
                    );
                    return;
                }
                (PanelLocation::Archive(_), _) => {
                    dialogs::show_error(
                        &self.window,
                        &t!("error.operation_unavailable"),
                        &t!("error.clipboard_paste_archive_unsupported"),
                    );
                    return;
                }
            }
        };

        let this = Rc::clone(self);
        let clear_cut = mode == FileClipboardMode::Cut;
        self.operations_controller()
            .start_prepared_operation(plan, move |result| {
                if result.is_ok() && clear_cut {
                    this.file_clipboard.borrow_mut().clear_if_cut();
                    clear_system_file_clipboard();
                }
            });
    }
}
