use super::PanelTabs;
use crate::domain::{Panel, PanelLocation};

fn panel(name: &str) -> Panel {
    Panel::new(
        PanelLocation::filesystem(std::path::PathBuf::from(format!("/{name}"))),
        Vec::new(),
        true,
    )
}

#[test]
fn open_and_close_tabs() {
    let mut tabs = PanelTabs::from_panel(panel("a"));
    assert_eq!(tabs.len(), 1);
    tabs.open(panel("b"));
    assert_eq!(tabs.len(), 2);
    assert_eq!(tabs.active().location.display_label(), "/b");
    assert!(tabs.close(tabs.active_index()));
    assert_eq!(tabs.len(), 1);
    assert!(!tabs.close(tabs.active_index()));
}

fn three_tabs_with_active(active: usize) -> PanelTabs {
    let mut tabs = PanelTabs::from_panel(panel("a"));
    tabs.open(panel("b"));
    tabs.open(panel("c"));
    assert!(tabs.activate(active) || active == 2);
    tabs
}

fn active_label(tabs: &PanelTabs) -> String {
    tabs.active().location.display_label()
}

#[test]
fn closing_a_tab_before_the_active_one_keeps_it_active() {
    let mut tabs = three_tabs_with_active(2);
    assert!(tabs.close(0));
    assert_eq!(tabs.len(), 2);
    assert_eq!(tabs.active_index(), 1);
    assert_eq!(active_label(&tabs), "/c");
}

#[test]
fn closing_a_tab_after_the_active_one_keeps_it_active() {
    let mut tabs = three_tabs_with_active(0);
    assert!(tabs.close(2));
    assert_eq!(tabs.active_index(), 0);
    assert_eq!(active_label(&tabs), "/a");
}

#[test]
fn closing_the_active_tab_activates_its_neighbour() {
    let mut tabs = three_tabs_with_active(1);
    assert!(tabs.close(1));
    assert_eq!(active_label(&tabs), "/c");

    let mut tabs = three_tabs_with_active(2);
    assert!(tabs.close(2));
    assert_eq!(active_label(&tabs), "/b");
}

#[test]
fn closing_needs_a_real_tab_and_another_one_to_stay() {
    let mut tabs = three_tabs_with_active(0);
    assert!(!tabs.close(3));
    assert_eq!(tabs.len(), 3);

    let mut single = PanelTabs::from_panel(panel("a"));
    assert!(!single.close(0));
    assert_eq!(single.len(), 1);
}

#[test]
fn next_prev_cycle() {
    let mut tabs = PanelTabs::from_panel(panel("a"));
    tabs.open(panel("b"));
    tabs.open(panel("c"));
    tabs.activate(0);
    assert!(tabs.next());
    assert_eq!(tabs.active_index(), 1);
    assert!(tabs.prev());
    assert_eq!(tabs.active_index(), 0);
}
