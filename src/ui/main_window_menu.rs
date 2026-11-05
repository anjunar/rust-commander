use gtk::gio;
use rust_i18n::t;

use crate::ui::dsl::{mount, tr};

/// The classic commander menu bar: Files / Commands / View / Help.
///
/// A `gio::Menu` holds plain strings, so the model is built again whenever
/// the locale changes.
pub(crate) fn menu_bar() {
    mount(
        gtk::PopoverMenuBar::from_model(None::<&gio::MenuModel>),
        |bar| {
            bar.css("app-menubar");
            bar.bind(tr!("menu.files"), |bar, _| {
                bar.set_menu_model(Some(&application_menu()));
            });
        },
    );
}

fn application_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append_submenu(Some(&t!("menu.files")), &files_menu());
    menu.append_submenu(Some(&t!("menu.commands")), &commands_menu());
    menu.append_submenu(Some(&t!("menu.view")), &view_menu());
    menu.append_submenu(Some(&t!("menu.help")), &help_menu());
    menu
}

fn files_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    append_item(&menu, &t!("menu.open"), "win.open");
    append_item(&menu, &t!("menu.view_file"), "win.view");
    append_item(&menu, &t!("menu.edit_file"), "win.edit");
    append_item(&menu, &t!("menu.properties"), "win.properties");
    append_item(&menu, &t!("menu.rename"), "win.rename");
    menu.append_section(None, &{
        let section = gio::Menu::new();
        append_item(&section, &t!("menu.new_file"), "win.create-text-file");
        append_item(&section, &t!("menu.mkdir"), "win.make-directory");
        section
    });
    menu.append_section(None, &{
        let section = gio::Menu::new();
        append_item(&section, &t!("menu.new_tab"), "win.new-tab");
        append_item(&section, &t!("menu.close_tab"), "win.close-tab");
        section
    });
    menu.append_section(None, &{
        let section = gio::Menu::new();
        append_item(&section, &t!("menu.favorites"), "win.favorites");
        append_item(&section, &t!("menu.connect_remote"), "win.connect-remote");
        section
    });
    menu.append_section(None, &{
        let section = gio::Menu::new();
        append_item(&section, &t!("menu.quit"), "win.quit");
        section
    });
    menu
}

fn commands_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    append_item(&menu, &t!("menu.copy"), "win.copy");
    append_item(&menu, &t!("menu.copy_as"), "win.copy-with-rename");
    append_item(&menu, &t!("menu.move"), "win.move-files");
    append_item(&menu, &t!("menu.delete"), "win.delete");
    append_item(&menu, &t!("menu.delete_permanent"), "win.delete-permanent");
    menu.append_section(None, &{
        let section = gio::Menu::new();
        append_item(&section, &t!("menu.clipboard_cut"), "win.clipboard-cut");
        append_item(&section, &t!("menu.clipboard_copy"), "win.clipboard-copy");
        append_item(&section, &t!("menu.clipboard_paste"), "win.clipboard-paste");
        section
    });
    menu.append_section(None, &{
        let section = gio::Menu::new();
        append_item(&section, &t!("menu.find_files"), "win.find-files");
        section
    });
    menu.append_section(None, &{
        let section = gio::Menu::new();
        append_item(&section, &t!("menu.mark_all"), "win.mark-all");
        append_item(&section, &t!("menu.switch_panel"), "win.switch-panel");
        append_item(&section, &t!("menu.swap_panels"), "win.swap-panels");
        section
    });
    menu.append_section(None, &{
        let section = gio::Menu::new();
        append_item(&section, &t!("menu.terminal"), "win.toggle-terminal");
        section
    });
    menu
}

fn view_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    append_item(&menu, &t!("menu.sort_name"), "win.sort-name");
    append_item(&menu, &t!("menu.sort_extension"), "win.sort-extension");
    append_item(&menu, &t!("menu.sort_date"), "win.sort-date");
    append_item(&menu, &t!("menu.sort_size"), "win.sort-size");
    menu.append_section(None, &{
        let section = gio::Menu::new();
        append_item(
            &section,
            &t!("menu.remember_folder_sort"),
            "win.remember-folder-sort",
        );
        append_item(
            &section,
            &t!("menu.forget_folder_sort"),
            "win.forget-folder-sort",
        );
        section
    });
    menu.append_section(None, &{
        let section = gio::Menu::new();
        append_item(&section, &t!("menu.refresh"), "win.refresh");
        append_item(
            &section,
            &t!("menu.toggle_hidden_files"),
            "win.toggle-hidden-files",
        );
        section
    });
    menu.append_section(None, &{
        let section = gio::Menu::new();
        append_item(&section, &t!("menu.drive_left"), "win.select-root-left");
        append_item(&section, &t!("menu.drive_right"), "win.select-root-right");
        append_item(&section, &t!("menu.go_root"), "win.go-location-root");
        section
    });
    menu
}

fn help_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    append_item(&menu, &t!("menu.settings"), "win.settings");
    menu.append_section(None, &{
        let section = gio::Menu::new();
        append_item(&section, &t!("menu.about"), "win.about");
        section
    });
    menu
}

fn append_item(menu: &gio::Menu, label: &str, action: &str) {
    let item = gio::MenuItem::new(Some(label), Some(action));
    menu.append_item(&item);
}
