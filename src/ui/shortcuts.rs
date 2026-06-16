use std::rc::Rc;

use gtk::{gio, prelude::*};

use crate::ui::main_window::MainWindow;

pub fn install(window: &Rc<MainWindow>, app: &gtk::Application) {
    add_action(window, "settings", MainWindow::handle_help);
    add_action(window, "view", MainWindow::handle_view);
    add_action(window, "rename", MainWindow::handle_rename);
    add_action(window, "console", MainWindow::handle_open_console);
    add_action(window, "edit", MainWindow::handle_edit);
    add_action(window, "copy", MainWindow::handle_copy);
    add_action(window, "move-files", MainWindow::handle_move);
    add_action(window, "delete", MainWindow::handle_delete);
    add_action(window, "switch-panel", MainWindow::handle_switch_panel);
    add_action(window, "open", MainWindow::handle_open_active);
    add_action(
        window,
        "toggle-terminal",
        MainWindow::handle_toggle_terminal,
    );
    add_action(window, "focus-terminal", MainWindow::handle_focus_terminal);
    add_action(
        window,
        "restart-terminal",
        MainWindow::handle_restart_terminal,
    );

    app.set_accels_for_action("win.view", &["F3"]);
    app.set_accels_for_action("win.settings", &["F1"]);
    app.set_accels_for_action("win.rename", &["F2"]);
    app.set_accels_for_action("win.edit", &["F4"]);
    app.set_accels_for_action("win.copy", &["F5"]);
    app.set_accels_for_action("win.move-files", &["F6"]);
    app.set_accels_for_action("win.delete", &["F8"]);
    app.set_accels_for_action("win.toggle-terminal", &["F9"]);
    app.set_accels_for_action("win.focus-terminal", &["<Primary>grave"]);
    app.set_accels_for_action("win.restart-terminal", &["<Primary><Shift>grave"]);
    app.set_accels_for_action("win.switch-panel", &["Tab"]);
    app.set_accels_for_action("win.open", &["Return", "KP_Enter"]);
}

fn add_action(window: &Rc<MainWindow>, name: &str, callback: fn(&Rc<MainWindow>)) {
    let action = gio::SimpleAction::new(name, None);
    let this = Rc::clone(window);
    action.connect_activate(move |_, _| {
        callback(&this);
    });
    window.window.add_action(&action);
}
