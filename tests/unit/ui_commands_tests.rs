use std::collections::HashSet;

use super::*;

#[test]
fn every_action_is_declared_once() {
    let mut seen = HashSet::new();
    for command in COMMANDS {
        assert!(seen.insert(command.action), "{}", command.action);
    }
}

#[test]
fn every_accelerator_belongs_to_one_command() {
    let mut seen = HashSet::new();
    for command in COMMANDS {
        for accel in command.accels {
            assert!(seen.insert(*accel), "{accel}");
        }
    }
}

#[test]
fn a_function_key_holds_one_command_per_mode() {
    let mut seen = HashSet::new();
    for slot in COMMANDS.iter().filter_map(|command| command.slot) {
        assert!((1..=10).contains(&slot.key), "{}", slot.label);
        assert!(
            seen.insert((format!("{:?}", slot.mode), slot.key)),
            "{}",
            slot.label
        );
    }
}

#[test]
fn every_function_key_has_a_plain_command() {
    for key in 1..=10 {
        assert!(bar_command(CommandBarMode::Normal, key).is_some(), "F{key}");
    }
}

#[test]
fn modifier_modes_only_fill_the_keys_they_use() {
    assert_eq!(
        bar_command(CommandBarMode::Shift, 5).map(|command| command.action),
        Some("copy-with-rename")
    );
    assert_eq!(
        bar_command(CommandBarMode::Ctrl, 3).map(|command| command.action),
        Some("sort-name")
    );
    assert!(bar_command(CommandBarMode::Alt, 5).is_none());
    assert!((1..=10).all(|key| bar_command(CommandBarMode::Hidden, key).is_none()));
}

#[test]
fn commands_are_found_by_action() {
    assert_eq!(command("quit").map(|command| command.action), Some("quit"));
    assert!(command("no-such-action").is_none());
}

#[test]
fn only_the_hidden_files_action_carries_a_state() {
    let toggles: Vec<&str> = COMMANDS
        .iter()
        .filter(|command| command.toggle)
        .map(|command| command.action)
        .collect();
    assert_eq!(toggles, vec!["toggle-hidden-files"]);
}
