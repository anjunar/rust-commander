#[cfg(not(target_os = "windows"))]
use std::{path::PathBuf, rc::Rc};

#[cfg(not(target_os = "windows"))]
use rust_i18n::t;

#[cfg(not(target_os = "windows"))]
use gtk::prelude::*;

#[cfg(not(target_os = "windows"))]
use crate::application::ActivePanel;

#[cfg(not(target_os = "windows"))]
use super::ContextMenuController;

#[cfg(not(target_os = "windows"))]
impl ContextMenuController {
    pub(super) fn show_unix_context_menu(
        &self,
        panel: ActivePanel,
        selected_paths: Vec<PathBuf>,
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

        if selected_paths.len() == 1 {
            self.append_action_button(
                &content,
                &popover,
                &t!("common.open"),
                Rc::clone(&self.actions.open),
            );
            self.append_action_button(
                &content,
                &popover,
                &t!("common.rename"),
                Rc::clone(&self.actions.rename),
            );
        }

        if !selected_paths.is_empty() {
            self.append_action_button(
                &content,
                &popover,
                &t!("operation.copy"),
                Rc::clone(&self.actions.copy),
            );
            self.append_action_button(
                &content,
                &popover,
                &t!("operation.move"),
                Rc::clone(&self.actions.move_entry),
            );
            self.append_action_button_with_css(
                &content,
                &popover,
                &t!("operation.delete"),
                Rc::clone(&self.actions.delete),
                Some("destructive-action"),
            );
        }

        self.append_action_button(
            &content,
            &popover,
            &t!("command.mkdir"),
            Rc::clone(&self.actions.mkdir),
        );

        if !selected_paths.is_empty() {
            content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

            self.append_path_action_button(
                &content,
                &popover,
                &t!("dialog.chmod_title"),
                selected_paths.clone(),
                Rc::clone(&self.actions.chmod),
            );
            self.append_path_action_button(
                &content,
                &popover,
                &t!("dialog.chown_title"),
                selected_paths.clone(),
                Rc::clone(&self.actions.chown),
            );
        }

        popover.set_child(Some(&content));
        popover.popup();
        self.runtime.unix_context_menu.replace(Some(popover));
    }

    fn append_action_button(
        &self,
        content: &gtk::Box,
        popover: &gtk::Popover,
        label: &str,
        action: Rc<dyn Fn()>,
    ) {
        self.append_action_button_with_css(content, popover, label, action, None);
    }

    fn append_action_button_with_css(
        &self,
        content: &gtk::Box,
        popover: &gtk::Popover,
        label: &str,
        action: Rc<dyn Fn()>,
        css_class: Option<&str>,
    ) {
        let menu = popover.clone();
        let controller = self.clone();
        let button = gtk::Button::with_label(label);
        if let Some(css_class) = css_class {
            button.add_css_class(css_class);
        }
        button.set_halign(gtk::Align::Fill);
        button.connect_clicked(move |_| {
            menu.popdown();
            controller.close_unix_context_menu();
            action();
        });
        content.append(&button);
    }

    fn append_path_action_button(
        &self,
        content: &gtk::Box,
        popover: &gtk::Popover,
        label: &str,
        paths: Vec<PathBuf>,
        action: Rc<dyn Fn(Vec<PathBuf>)>,
    ) {
        let menu = popover.clone();
        let controller = self.clone();
        let button = gtk::Button::with_label(label);
        button.set_halign(gtk::Align::Fill);
        button.connect_clicked(move |_| {
            menu.popdown();
            controller.close_unix_context_menu();
            action(paths.clone());
        });
        content.append(&button);
    }

    pub(super) fn close_unix_context_menu(&self) {
        if let Some(popover) = self.runtime.unix_context_menu.borrow_mut().take() {
            popover.popdown();
            popover.unparent();
        }
    }
}
