use std::{cell::RefCell, path::PathBuf, rc::Rc};

use gtk::prelude::*;

use crate::ui::{
    terminal_controller::{TerminalAction, TerminalCommand, TerminalController},
    terminal_state::TerminalState,
};

#[derive(Clone)]
pub struct TerminalDock {
    pub root: gtk::Box,
    title_label: gtk::Label,
    cwd_label: gtk::Label,
    restart_button: gtk::Button,
    clear_button: gtk::Button,
    close_button: gtk::Button,
    controller: Rc<TerminalController>,
}

impl TerminalDock {
    pub fn new(initial_dir: PathBuf) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.set_hexpand(true);
        root.set_vexpand(true);
        root.add_css_class("terminal-dock");
        root.set_visible(false);

        let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        toolbar.add_css_class("terminal-toolbar");
        toolbar.set_margin_top(8);
        toolbar.set_margin_bottom(8);
        toolbar.set_margin_start(10);
        toolbar.set_margin_end(10);

        let title_label = gtk::Label::new(Some("Terminal"));
        title_label.set_xalign(0.0);
        title_label.add_css_class("terminal-title");
        toolbar.append(&title_label);

        let cwd_label = gtk::Label::new(None);
        cwd_label.set_xalign(0.0);
        cwd_label.set_hexpand(true);
        cwd_label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        cwd_label.add_css_class("terminal-cwd");
        toolbar.append(&cwd_label);

        let restart_button = gtk::Button::with_label("Restart");
        let clear_button = gtk::Button::with_label("Clear");
        let close_button = gtk::Button::with_label("Close");
        restart_button.add_css_class("terminal-button");
        clear_button.add_css_class("terminal-button");
        close_button.add_css_class("terminal-button");
        toolbar.append(&restart_button);
        toolbar.append(&clear_button);
        toolbar.append(&close_button);

        root.append(&toolbar);

        let controller = Rc::new(TerminalController::new(initial_dir));
        root.append(&controller.widget());

        let dock = Self {
            root,
            title_label,
            cwd_label,
            restart_button,
            clear_button,
            close_button,
            controller,
        };

        dock.refresh_toolbar();
        dock
    }

    pub fn state(&self) -> Rc<RefCell<TerminalState>> {
        self.controller.state()
    }

    pub fn is_supported(&self) -> bool {
        self.controller.is_supported()
    }

    pub fn has_focus(&self) -> bool {
        self.controller.has_focus()
    }

    pub fn set_panel_dir(&self, path: PathBuf) {
        self.controller.set_panel_dir(&path);
        let state = self.state();
        let mut state = state.borrow_mut();
        state.last_panel_dir = path;
        drop(state);
        self.refresh_toolbar();
    }

    pub fn toggle(&self) -> TerminalAction {
        let action = self.controller.run(TerminalCommand::ToggleVisibility);
        self.sync_visibility();
        self.refresh_toolbar();
        action
    }

    pub fn focus_terminal(&self) -> TerminalAction {
        let action = self.controller.run(TerminalCommand::Focus);
        self.sync_visibility();
        self.refresh_toolbar();
        action
    }

    pub fn restart_in_panel_dir(&self) -> TerminalAction {
        let action = self.controller.run(TerminalCommand::RestartInPanelDir);
        self.sync_visibility();
        self.refresh_toolbar();
        action
    }

    pub fn clear(&self) {
        let _ = self.controller.run(TerminalCommand::Clear);
    }

    pub fn close(&self) -> TerminalAction {
        let _ = self.controller.run(TerminalCommand::Hide);
        self.sync_visibility();
        self.refresh_toolbar();
        TerminalAction::FocusPanels
    }

    pub fn connect_focus_return<F>(&self, f: F)
    where
        F: Fn() + 'static,
    {
        self.controller.connect_escape_to_focus_panels(f);
    }

    pub fn connect_buttons<F>(&self, on_action: F)
    where
        F: Fn(TerminalAction) + Clone + 'static,
    {
        {
            let dock = self.clone();
            let on_action = on_action.clone();
            self.restart_button.connect_clicked(move |_| {
                on_action(dock.restart_in_panel_dir());
            });
        }

        {
            let dock = self.clone();
            self.clear_button.connect_clicked(move |_| {
                dock.clear();
            });
        }

        {
            let dock = self.clone();
            let on_action = on_action.clone();
            self.close_button.connect_clicked(move |_| {
                on_action(dock.close());
            });
        }
    }

    pub fn sync_visibility(&self) {
        let visible = self.state().borrow().visible;
        self.root.set_visible(visible);
        self.clear_button
            .set_sensitive(self.controller.is_supported());
    }

    pub fn refresh_toolbar(&self) {
        if self.controller.is_supported() {
            self.title_label.set_label("Terminal");
            let cwd = self.controller.current_working_dir();
            self.cwd_label.set_label(&cwd.display().to_string());
        } else {
            self.title_label
                .set_label("Terminal (Windows backend pending)");
            let cwd = self.state().borrow().last_panel_dir.clone();
            self.cwd_label
                .set_label(&format!("Panel path: {}", cwd.display()));
        }
    }
}
