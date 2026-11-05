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
    assert!(tabs.close_active());
    assert_eq!(tabs.len(), 1);
    assert!(!tabs.close_active());
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
