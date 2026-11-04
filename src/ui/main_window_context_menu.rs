use std::{cell::RefCell, path::PathBuf, rc::Rc};

use rust_i18n::t;

use crate::application::{ActivePanel, Commander, SharedPlatformPort};
#[cfg(target_os = "windows")]
use crate::ui::dialogs;

use super::hosts::ViewHost;

#[path = "main_window_context_menu_unix.rs"]
mod unix;

/// What the in-app context menu can do. Windows shows the native shell menu
/// instead, so there it is never built.
#[cfg_attr(target_os = "windows", allow(dead_code))]
#[derive(Clone)]
pub struct ContextMenuActions {
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
    commander: Rc<RefCell<Commander>>,
    _platform_port: SharedPlatformPort,
    #[cfg(not(target_os = "windows"))]
    actions: ContextMenuActions,
}

impl ContextMenuController {
    pub fn new(
        host: Rc<dyn ViewHost>,
        window: gtk::ApplicationWindow,
        commander: Rc<RefCell<Commander>>,
        platform_port: SharedPlatformPort,
        #[cfg(not(target_os = "windows"))] actions: ContextMenuActions,
    ) -> Self {
        Self {
            host,
            _window: window,
            commander,
            _platform_port: platform_port,
            #[cfg(not(target_os = "windows"))]
            actions,
        }
    }

    /// Opens the context menu for a right click at `(x, y)` on `anchor`.
    ///
    /// A click on an unselected row selects it first; a click inside the
    /// selection keeps the selection, so a menu can act on several entries.
    pub fn handle_panel_context_menu(
        &self,
        panel: ActivePanel,
        clicked_index: Option<usize>,
        anchor: &gtk::Widget,
        x: f64,
        y: f64,
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
            let _ = (anchor, x, y);
            if let Err(error) = self
                ._platform_port
                .show_context_menu(directory, selected_paths)
            {
                self.show_command_failed(error);
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = &directory;
            crate::ui::dsl::popup_menu(anchor, x, y, |menu| {
                unix::compose(menu, &self.actions, &selected_paths)
            });
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
