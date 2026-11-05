use crate::domain::panel::Panel;

/// Unique id for a tab within one side (left or right). Survives reorder/close of others.
pub type TabId = u64;

#[derive(Clone, Debug)]
pub struct PanelTab {
    pub id: TabId,
    pub panel: Panel,
}

/// Tabs for one commander side. Always at least one tab.
#[derive(Clone, Debug)]
pub struct PanelTabs {
    tabs: Vec<PanelTab>,
    active: usize,
    next_id: TabId,
}

impl PanelTabs {
    pub fn from_panel(panel: Panel) -> Self {
        Self {
            tabs: vec![PanelTab { id: 1, panel }],
            active: 0,
            next_id: 2,
        }
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn active_id(&self) -> TabId {
        self.tabs[self.active].id
    }

    pub fn active(&self) -> &Panel {
        &self.tabs[self.active].panel
    }

    pub fn active_mut(&mut self) -> &mut Panel {
        &mut self.tabs[self.active].panel
    }

    pub fn tabs(&self) -> &[PanelTab] {
        &self.tabs
    }

    pub fn iter_panels_mut(&mut self) -> impl Iterator<Item = &mut Panel> {
        self.tabs.iter_mut().map(|tab| &mut tab.panel)
    }

    /// Activate tab by index. Returns `true` when the active tab changed.
    pub fn activate(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() || index == self.active {
            return false;
        }
        self.active = index;
        true
    }

    /// Activate tab by id. Returns `true` when the active tab changed.
    #[allow(dead_code)]
    pub fn activate_id(&mut self, id: TabId) -> bool {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return false;
        };
        self.activate(index)
    }

    /// Open a new tab after the active one and make it active.
    pub fn open(&mut self, panel: Panel) -> TabId {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let insert_at = self.active + 1;
        self.tabs.insert(insert_at, PanelTab { id, panel });
        self.active = insert_at;
        id
    }

    /// Close the active tab. Keeps at least one tab. Returns whether a tab was closed.
    pub fn close_active(&mut self) -> bool {
        if self.tabs.len() <= 1 {
            return false;
        }
        self.tabs.remove(self.active);
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        }
        true
    }

    pub fn next(&mut self) -> bool {
        if self.tabs.len() <= 1 {
            return false;
        }
        self.active = (self.active + 1) % self.tabs.len();
        true
    }

    pub fn prev(&mut self) -> bool {
        if self.tabs.len() <= 1 {
            return false;
        }
        self.active = if self.active == 0 {
            self.tabs.len() - 1
        } else {
            self.active - 1
        };
        true
    }

    #[allow(dead_code)]
    pub fn contains_id(&self, id: TabId) -> bool {
        self.tabs.iter().any(|tab| tab.id == id)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/domain_panel_tabs_tests.rs"]
mod tests;
