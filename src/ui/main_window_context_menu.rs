use std::{cell::RefCell, path::PathBuf, rc::Rc};

use rust_i18n::t;

use crate::application::{ActivePanel, Commander, SharedPlatformPort};
#[cfg(target_os = "windows")]
use crate::ui::dialogs;

use super::hosts::ViewHost;

#[path = "main_window_context_menu_unix.rs"]
mod unix;

#[derive(Clone)]
pub struct ContextMenuRuntime {
    #[cfg(not(target_os = "windows"))]
    pub unix_context_menu: Rc<RefCell<Option<gtk::Popover>>>,
}

impl ContextMenuRuntime {
    pub fn new() -> Self {
        Self {
            #[cfg(not(target_os = "windows"))]
            unix_context_menu: Rc::new(RefCell::new(None)),
        }
    }
}

#[cfg(not(target_os = "windows"))]
#[derive(Clone)]
pub struct UnixContextMenuActions {
    pub open: Rc<dyn Fn()>,
    pub rename: Rc<dyn Fn()>,
    pub copy: Rc<dyn Fn()>,
    pub move_entry: Rc<dyn Fn()>,
    pub delete: Rc<dyn Fn()>,
    pub mkdir: Rc<dyn Fn()>,
    pub chmod: Rc<dyn Fn(Vec<PathBuf>)>,
    pub chown: Rc<dyn Fn(Vec<PathBuf>)>,
}

#[derive(Clone)]
pub struct ContextMenuController {
    host: Rc<dyn ViewHost>,
    _window: gtk::ApplicationWindow,
    #[cfg(not(target_os = "windows"))]
    left_panel_root: gtk::Box,
    #[cfg(not(target_os = "windows"))]
    right_panel_root: gtk::Box,
    commander: Rc<RefCell<Commander>>,
    platform_port: SharedPlatformPort,
    #[cfg(not(target_os = "windows"))]
    runtime: ContextMenuRuntime,
    #[cfg(not(target_os = "windows"))]
    actions: UnixContextMenuActions,
}

impl ContextMenuController {
    pub fn new(
        host: Rc<dyn ViewHost>,
        window: gtk::ApplicationWindow,
        #[cfg(not(target_os = "windows"))] left_panel_root: gtk::Box,
        #[cfg(not(target_os = "windows"))] right_panel_root: gtk::Box,
        commander: Rc<RefCell<Commander>>,
        platform_port: SharedPlatformPort,
        _runtime: ContextMenuRuntime,
        #[cfg(not(target_os = "windows"))] actions: UnixContextMenuActions,
    ) -> Self {
        Self {
            host,
            _window: window,
            #[cfg(not(target_os = "windows"))]
            left_panel_root,
            #[cfg(not(target_os = "windows"))]
            right_panel_root,
            commander,
            platform_port,
            #[cfg(not(target_os = "windows"))]
            runtime: _runtime,
            #[cfg(not(target_os = "windows"))]
            actions,
        }
    }

    pub fn handle_panel_context_menu(
        &self,
        panel: ActivePanel,
        clicked_index: Option<usize>,
        _x: f64,
        _y: f64,
    ) {
        let ((directory, selected_paths), update) = {
            let mut commander = self.commander.borrow_mut();
            let mut update = commander.set_active_panel(panel);
            if let Some(index) = clicked_index {
                let keep_multi_selection = commander
                    .state()
                    .panel(panel)
                    .selection_indices()
                    .contains(&index);
                if !keep_multi_selection {
                    update = commander.select_single(panel, index);
                }
            }
            let panel_state = commander.state().panel(panel);
            let Some(directory) = panel_state.location.filesystem_path().map(PathBuf::from) else {
                self.host.show_error(
                    &t!("error.command_failed"),
                    "The native context menu is currently only available in filesystem views.",
                );
                return;
            };

            let selected_paths = panel_state
                .selected_items()
                .into_iter()
                .filter(|item| item.archive_path.is_none())
                .filter_map(|item| item.filesystem_path)
                .collect::<Vec<_>>();

            ((directory, selected_paths), update)
        };

        self.host.apply_update(update);

        #[cfg(target_os = "windows")]
        {
            if let Err(error) = self
                .platform_port
                .show_context_menu(directory, selected_paths)
            {
                self.show_command_failed(error);
                return;
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            self.show_unix_context_menu(panel, selected_paths, _x, _y);
        }
    }

    #[cfg(target_os = "windows")]
    fn show_command_failed(&self, error: impl std::fmt::Display) {
        let error = error.to_string();
        self.host
            .set_status(t!("status.command_failed", error = error.as_str()).into_owned());
        dialogs::show_error(&self._window, &t!("error.command_failed"), &error);
    }
}
