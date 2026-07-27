use gtk::prelude::*;
use rust_i18n::t;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandBarLabelMode {
    Normal,
    Alt,
    Ctrl,
    Shift,
    Hidden,
}

#[derive(Clone, Debug)]
struct CommandBarLabel {
    key: String,
    caption: String,
}

pub fn build_command_bar() -> gtk::Box {
    let command_bar = gtk::Box::new(gtk::Orientation::Horizontal, 3);
    command_bar.add_css_class("command-bar");
    command_bar.set_homogeneous(true);

    for label in command_bar_labels(CommandBarLabelMode::Normal) {
        command_bar.append(&build_command_button(&label));
    }

    command_bar
}

pub(crate) fn apply_command_bar_labels(command_bar: &gtk::Box, mode: CommandBarLabelMode) {
    let labels = command_bar_labels(mode);
    let mut index = 0usize;
    let mut child = command_bar.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if let Ok(button) = widget.downcast::<gtk::Button>() {
            if let Some(label) = labels.get(index) {
                set_command_button_label(&button, label);
            }
            index += 1;
        }
    }
}

fn build_command_button(label: &CommandBarLabel) -> gtk::Button {
    let button = gtk::Button::new();
    button.add_css_class("command-button");
    button.set_focus_on_click(false);
    button.set_can_focus(false);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row.set_halign(gtk::Align::Center);
    row.set_valign(gtk::Align::Center);
    row.set_hexpand(true);

    let key = gtk::Label::new(None);
    key.add_css_class("command-key");
    key.set_valign(gtk::Align::Center);

    let caption = gtk::Label::new(None);
    caption.add_css_class("command-caption");
    caption.set_valign(gtk::Align::Center);
    caption.set_xalign(0.0);
    caption.set_ellipsize(gtk::pango::EllipsizeMode::End);

    row.append(&key);
    row.append(&caption);
    button.set_child(Some(&row));
    set_command_button_label(&button, label);
    button
}

fn set_command_button_label(button: &gtk::Button, label: &CommandBarLabel) {
    let idle = label.key.is_empty() && label.caption.is_empty();
    if idle {
        button.add_css_class("command-button-idle");
    } else {
        button.remove_css_class("command-button-idle");
    }

    let Some(child) = button.child() else {
        return;
    };
    let Ok(row) = child.downcast::<gtk::Box>() else {
        return;
    };

    let mut slot = 0usize;
    let mut widget = row.first_child();
    while let Some(current) = widget {
        widget = current.next_sibling();
        if let Ok(label_widget) = current.downcast::<gtk::Label>() {
            match slot {
                0 => {
                    label_widget.set_text(&label.key);
                    label_widget.set_visible(!label.key.is_empty());
                }
                1 => {
                    label_widget.set_text(&label.caption);
                    label_widget.set_visible(!label.caption.is_empty());
                }
                _ => {}
            }
            slot += 1;
        }
    }
}

fn command_bar_labels(mode: CommandBarLabelMode) -> Vec<CommandBarLabel> {
    let raw = match mode {
        CommandBarLabelMode::Normal => vec![
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
        ],
        CommandBarLabelMode::Alt => vec![
            t!("command.drive_left").into_owned(),
            t!("command.drive_right").into_owned(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ],
        CommandBarLabelMode::Ctrl => vec![
            String::new(),
            String::new(),
            t!("command.sort_name").into_owned(),
            t!("command.sort_extension").into_owned(),
            t!("command.sort_date").into_owned(),
            t!("command.sort_size").into_owned(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ],
        CommandBarLabelMode::Shift => vec![
            String::new(),
            String::new(),
            String::new(),
            t!("command.new_file").into_owned(),
            t!("command.copy_as").into_owned(),
            String::new(),
            String::new(),
            t!("command.delete_permanent").into_owned(),
            String::new(),
            String::new(),
        ],
        CommandBarLabelMode::Hidden => vec![String::new(); 10],
    };

    raw.into_iter()
        .map(|text| split_command_label(&text))
        .collect()
}

fn split_command_label(full: &str) -> CommandBarLabel {
    let trimmed = full.trim();
    if trimmed.is_empty() {
        return CommandBarLabel {
            key: String::new(),
            caption: String::new(),
        };
    }

    match trimmed.split_once(' ') {
        Some((key, rest)) if is_function_key(key) => CommandBarLabel {
            key: key.to_string(),
            caption: rest.trim().to_string(),
        },
        _ if is_function_key(trimmed) => CommandBarLabel {
            key: trimmed.to_string(),
            caption: String::new(),
        },
        _ => CommandBarLabel {
            key: String::new(),
            caption: trimmed.to_string(),
        },
    }
}

fn is_function_key(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('F') | Some('f'))
        && chars.next().is_some_and(|ch| ch.is_ascii_digit())
        && chars.all(|ch| ch.is_ascii_digit())
}
