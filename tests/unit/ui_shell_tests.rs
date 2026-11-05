use gtk::gdk::{Key, ModifierType};

use super::*;

fn split(label: &str) -> (String, String) {
    split_command_label(label)
}

#[test]
fn command_labels_split_into_key_and_caption() {
    assert_eq!(split("F5 Copy"), ("F5".to_string(), "Copy".to_string()));
    assert_eq!(
        split("  F10   Quit now "),
        ("F10".to_string(), "Quit now".to_string())
    );
}

#[test]
fn command_labels_without_a_function_key_are_all_caption() {
    assert_eq!(split("Copy"), (String::new(), "Copy".to_string()));
    assert_eq!(split("File list"), (String::new(), "File list".to_string()));
    assert_eq!(split("F"), (String::new(), "F".to_string()));
}

#[test]
fn a_bare_function_key_has_no_caption() {
    assert_eq!(split("F7"), ("F7".to_string(), String::new()));
}

#[test]
fn an_empty_label_stays_empty() {
    assert_eq!(split(""), (String::new(), String::new()));
    assert_eq!(split("   "), (String::new(), String::new()));
}

#[test]
fn the_bar_mode_follows_the_modifiers() {
    let none = ModifierType::empty();
    assert_eq!(command_bar_mode(none), CommandBarMode::Normal);
    assert_eq!(
        command_bar_mode(ModifierType::ALT_MASK),
        CommandBarMode::Alt
    );
    assert_eq!(
        command_bar_mode(ModifierType::CONTROL_MASK),
        CommandBarMode::Ctrl
    );
    assert_eq!(
        command_bar_mode(ModifierType::SHIFT_MASK),
        CommandBarMode::Shift
    );
}

#[test]
fn control_with_shift_hides_the_labels() {
    assert_eq!(
        command_bar_mode(ModifierType::CONTROL_MASK | ModifierType::SHIFT_MASK),
        CommandBarMode::Hidden
    );
}

#[test]
fn the_key_being_pressed_counts_as_held() {
    let modifiers = effective_modifiers(Key::Shift_L, ModifierType::empty(), true);
    assert_eq!(command_bar_mode(modifiers), CommandBarMode::Shift);
}

#[test]
fn the_key_being_released_no_longer_counts() {
    let modifiers = effective_modifiers(Key::Control_R, ModifierType::CONTROL_MASK, false);
    assert_eq!(command_bar_mode(modifiers), CommandBarMode::Normal);
}

#[test]
fn other_keys_leave_the_modifiers_alone() {
    let modifiers = effective_modifiers(Key::a, ModifierType::ALT_MASK, true);
    assert_eq!(modifiers, ModifierType::ALT_MASK);
}
