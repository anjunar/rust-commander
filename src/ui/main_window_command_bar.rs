use gtk::prelude::*;
use rust_i18n::t;

pub fn build_command_bar() -> gtk::Box {
    let command_bar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    command_bar.add_css_class("command-bar");
    command_bar.set_homogeneous(true);

    for label in command_bar_labels() {
        let button = gtk::Button::with_label(&label);
        button.add_css_class("command-button");
        command_bar.append(&button);
    }

    command_bar
}

pub(crate) fn command_bar_labels() -> Vec<String> {
    vec![
        t!("command.settings").into_owned(),
        t!("command.rename").into_owned(),
        t!("command.view").into_owned(),
        t!("command.edit").into_owned(),
        t!("command.copy").into_owned(),
        t!("command.move").into_owned(),
        t!("command.mkdir").into_owned(),
        t!("command.delete").into_owned(),
        t!("command.terminal").into_owned(),
        t!("command.quit").into_owned(),
    ]
}
