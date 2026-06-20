use std::{cell::RefCell, path::PathBuf, rc::Rc};

use rust_i18n::t;

#[cfg(target_os = "windows")]
use crate::ui::dialogs;
use crate::{
    application::{ActivePanel, Commander},
    platform::ContextMenuRequest,
};

use super::hosts::ViewHost;

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
        let (request, update) = {
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

            (
                ContextMenuRequest {
                    directory,
                    selected_paths,
                },
                update,
            )
        };

        self.host.apply_update(update);

        #[cfg(target_os = "windows")]
        {
            if let Err(error) = crate::platform::show_context_menu(&request) {
                self.show_command_failed(error);
                return;
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            self.show_unix_context_menu(panel, request, _x, _y);
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn show_unix_context_menu(
        &self,
        panel: ActivePanel,
        request: ContextMenuRequest,
        x: f64,
        y: f64,
    ) {
        self.close_unix_context_menu();

        let panel_root = match panel {
            ActivePanel::Left => self.left_panel_root.clone(),
            ActivePanel::Right => self.right_panel_root.clone(),
        };
        let popover = gtk::Popover::new();
        popover.set_parent(&panel_root);
        popover.set_has_arrow(false);
        popover.set_position(gtk::PositionType::Bottom);
        popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(
            x.round() as i32,
            y.round() as i32,
            1,
            1,
        )));

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.set_margin_top(6);
        content.set_margin_bottom(6);
        content.set_margin_start(6);
        content.set_margin_end(6);

        if request.selected_paths.len() == 1 {
            let menu = popover.clone();
            let open_action = Rc::clone(&self.actions.open);
            let controller = self.clone();
            let button = gtk::Button::with_label(&t!("common.open"));
            button.set_halign(gtk::Align::Fill);
            button.connect_clicked(move |_| {
                menu.popdown();
                controller.close_unix_context_menu();
                open_action();
            });
            content.append(&button);
        }

        if request.selected_paths.len() == 1 {
            let menu = popover.clone();
            let rename_action = Rc::clone(&self.actions.rename);
            let controller = self.clone();
            let button = gtk::Button::with_label(&t!("common.rename"));
            button.set_halign(gtk::Align::Fill);
            button.connect_clicked(move |_| {
                menu.popdown();
                controller.close_unix_context_menu();
                rename_action();
            });
            content.append(&button);
        }

        if !request.selected_paths.is_empty() {
            let menu = popover.clone();
            let copy_action = Rc::clone(&self.actions.copy);
            let controller = self.clone();
            let button = gtk::Button::with_label(&t!("operation.copy"));
            button.set_halign(gtk::Align::Fill);
            button.connect_clicked(move |_| {
                menu.popdown();
                controller.close_unix_context_menu();
                copy_action();
            });
            content.append(&button);

            let menu = popover.clone();
            let move_action = Rc::clone(&self.actions.move_entry);
            let controller = self.clone();
            let button = gtk::Button::with_label(&t!("operation.move"));
            button.set_halign(gtk::Align::Fill);
            button.connect_clicked(move |_| {
                menu.popdown();
                controller.close_unix_context_menu();
                move_action();
            });
            content.append(&button);

            let menu = popover.clone();
            let delete_action = Rc::clone(&self.actions.delete);
            let controller = self.clone();
            let button = gtk::Button::with_label(&t!("operation.delete"));
            button.add_css_class("destructive-action");
            button.set_halign(gtk::Align::Fill);
            button.connect_clicked(move |_| {
                menu.popdown();
                controller.close_unix_context_menu();
                delete_action();
            });
            content.append(&button);
        }

        {
            let menu = popover.clone();
            let mkdir_action = Rc::clone(&self.actions.mkdir);
            let controller = self.clone();
            let button = gtk::Button::with_label(&t!("command.mkdir"));
            button.set_halign(gtk::Align::Fill);
            button.connect_clicked(move |_| {
                menu.popdown();
                controller.close_unix_context_menu();
                mkdir_action();
            });
            content.append(&button);
        }

        if !request.selected_paths.is_empty() {
            content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

            let chmod_paths = request.selected_paths.clone();
            let menu = popover.clone();
            let chmod_action = Rc::clone(&self.actions.chmod);
            let controller = self.clone();
            let button = gtk::Button::with_label(&t!("dialog.chmod_title"));
            button.set_halign(gtk::Align::Fill);
            button.connect_clicked(move |_| {
                menu.popdown();
                controller.close_unix_context_menu();
                chmod_action(chmod_paths.clone());
            });
            content.append(&button);

            let chown_paths = request.selected_paths.clone();
            let menu = popover.clone();
            let chown_action = Rc::clone(&self.actions.chown);
            let controller = self.clone();
            let button = gtk::Button::with_label(&t!("dialog.chown_title"));
            button.set_halign(gtk::Align::Fill);
            button.connect_clicked(move |_| {
                menu.popdown();
                controller.close_unix_context_menu();
                chown_action(chown_paths.clone());
            });
            content.append(&button);
        }

        popover.set_child(Some(&content));
        popover.popup();
        self.runtime.unix_context_menu.replace(Some(popover));
    }

    #[cfg(not(target_os = "windows"))]
    fn close_unix_context_menu(&self) {
        if let Some(popover) = self.runtime.unix_context_menu.borrow_mut().take() {
            popover.popdown();
            popover.unparent();
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
