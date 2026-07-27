use std::rc::Rc;

use gtk::{gio, prelude::*};

use crate::ui::main_window::MainWindow;

pub fn install(window: &Rc<MainWindow>, app: &gtk::Application) {
    add_action(window, "settings", MainWindow::handle_help);
    add_action(window, "about", MainWindow::handle_about);
    add_action(window, "view", MainWindow::handle_view);
    add_action(window, "properties", MainWindow::handle_properties);
    add_action(window, "rename", MainWindow::handle_rename);
    add_action(
        window,
        "select-root-left",
        MainWindow::handle_select_root_left,
    );
    add_action(
        window,
        "select-root-right",
        MainWindow::handle_select_root_right,
    );
    add_action(window, "console", MainWindow::handle_open_console);
    add_action(window, "edit", MainWindow::handle_edit);
    add_action(
        window,
        "create-text-file",
        MainWindow::handle_create_text_file,
    );
    add_action(window, "copy", MainWindow::handle_copy);
    add_action(
        window,
        "copy-with-rename",
        MainWindow::handle_copy_with_rename,
    );
    add_action(window, "move-files", MainWindow::handle_move);
    add_action(window, "clipboard-copy", MainWindow::handle_clipboard_copy);
    add_action(window, "clipboard-cut", MainWindow::handle_clipboard_cut);
    add_action(
        window,
        "clipboard-paste",
        MainWindow::handle_clipboard_paste,
    );
    add_action(window, "make-directory", MainWindow::handle_make_directory);
    add_action(window, "delete", MainWindow::handle_delete);
    add_action(
        window,
        "delete-permanent",
        MainWindow::handle_delete_permanent,
    );
    add_action(window, "connect-remote", MainWindow::handle_connect_remote);
    add_action(window, "quit", MainWindow::handle_quit);
    add_action(window, "switch-panel", MainWindow::handle_switch_panel);
    add_action(window, "swap-panels", MainWindow::handle_swap_panels);
    add_action(window, "mark-all", MainWindow::handle_mark_all);
    add_action(window, "new-tab", MainWindow::handle_new_tab);
    add_action(window, "close-tab", MainWindow::handle_close_tab);
    add_action(window, "next-tab", MainWindow::handle_next_tab);
    add_action(window, "prev-tab", MainWindow::handle_prev_tab);
    add_action(
        window,
        "focus-active-panel",
        MainWindow::handle_focus_active_panel,
    );
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
    add_action(window, "sort-name", MainWindow::handle_sort_by_name);
    add_action(
        window,
        "sort-extension",
        MainWindow::handle_sort_by_extension,
    );
    add_action(window, "sort-date", MainWindow::handle_sort_by_date);
    add_action(window, "sort-size", MainWindow::handle_sort_by_size);
    add_action(
        window,
        "remember-folder-sort",
        MainWindow::handle_remember_folder_sort,
    );
    add_action(
        window,
        "forget-folder-sort",
        MainWindow::handle_forget_folder_sort,
    );
    add_action(window, "favorites", MainWindow::handle_favorites);
    add_action(window, "find-files", MainWindow::handle_find_files);
    add_action(window, "refresh", MainWindow::handle_refresh);
    add_action(
        window,
        "go-location-root",
        MainWindow::handle_go_location_root,
    );
    add_stateful_bool_action(
        window,
        "toggle-hidden-files",
        window.show_hidden_files_enabled(),
        MainWindow::handle_toggle_hidden_files,
    );

    app.set_accels_for_action("win.view", &["F3"]);
    app.set_accels_for_action("win.properties", &["<Alt>Return"]);
    app.set_accels_for_action("win.settings", &["F1"]);
    app.set_accels_for_action("win.rename", &["F2"]);
    app.set_accels_for_action("win.select-root-left", &["<Alt>F1"]);
    app.set_accels_for_action("win.select-root-right", &["<Alt>F2"]);
    app.set_accels_for_action("win.edit", &["F4"]);
    app.set_accels_for_action("win.create-text-file", &["<Shift>F4"]);
    app.set_accels_for_action("win.copy", &["F5"]);
    app.set_accels_for_action("win.copy-with-rename", &["<Shift>F5"]);
    app.set_accels_for_action("win.move-files", &["F6"]);
    app.set_accels_for_action("win.clipboard-copy", &["<Primary>c"]);
    app.set_accels_for_action("win.clipboard-cut", &["<Primary>x"]);
    app.set_accels_for_action("win.clipboard-paste", &["<Primary>v"]);
    app.set_accels_for_action("win.make-directory", &["F7"]);
    app.set_accels_for_action("win.delete", &["F8", "Delete"]);
    app.set_accels_for_action("win.delete-permanent", &["<Shift>F8", "<Shift>Delete"]);
    app.set_accels_for_action("win.toggle-terminal", &["F9"]);
    app.set_accels_for_action("win.quit", &["F10"]);
    app.set_accels_for_action("win.connect-remote", &["F11"]);
    app.set_accels_for_action("win.sort-name", &["<Primary>F3"]);
    app.set_accels_for_action("win.sort-extension", &["<Primary>F4"]);
    app.set_accels_for_action("win.sort-date", &["<Primary>F5"]);
    app.set_accels_for_action("win.sort-size", &["<Primary>F6"]);
    app.set_accels_for_action("win.focus-terminal", &["<Primary>grave"]);
    app.set_accels_for_action("win.restart-terminal", &["<Primary><Shift>grave"]);
    app.set_accels_for_action("win.favorites", &["<Primary>d"]);
    app.set_accels_for_action("win.find-files", &["<Alt>F7", "<Primary><Shift>f"]);
    app.set_accels_for_action("win.refresh", &["<Primary>r"]);
    app.set_accels_for_action("win.go-location-root", &["<Primary>backslash"]);
    app.set_accels_for_action("win.toggle-hidden-files", &["<Primary>period"]);
    app.set_accels_for_action("win.switch-panel", &["Tab"]);
    app.set_accels_for_action("win.swap-panels", &["<Primary>u"]);
    app.set_accels_for_action("win.mark-all", &["<Primary>a"]);
    app.set_accels_for_action("win.new-tab", &["<Primary>t"]);
    app.set_accels_for_action("win.close-tab", &["<Primary>w"]);
    app.set_accels_for_action("win.next-tab", &["<Primary>Tab", "<Primary>Page_Down"]);
    app.set_accels_for_action("win.prev-tab", &["<Primary><Shift>Tab", "<Primary>Page_Up"]);
}

fn add_action(window: &Rc<MainWindow>, name: &str, callback: fn(&Rc<MainWindow>)) {
    let action = gio::SimpleAction::new(name, None);
    let this = Rc::clone(window);
    action.connect_activate(move |_, _| {
        callback(&this);
    });
    window.window.add_action(&action);
}

fn add_stateful_bool_action(
    window: &Rc<MainWindow>,
    name: &str,
    initial: bool,
    callback: fn(&Rc<MainWindow>),
) {
    let action = gio::SimpleAction::new_stateful(name, None, &initial.to_variant());
    let this = Rc::clone(window);
    action.connect_activate(move |_, _| {
        callback(&this);
    });
    window.window.add_action(&action);
}
