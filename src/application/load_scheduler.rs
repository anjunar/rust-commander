use crate::application::{ActivePanel, NavigationRequest};

#[derive(Clone, Copy, Debug, Default)]
struct PanelLoadRuntime {
    visible_generation: u64,
    in_flight_generation: Option<u64>,
    dirty: bool,
}

#[derive(Clone, Debug, Default)]
pub struct LoadScheduler {
    next_generation: u64,
    left: PanelLoadRuntime,
    right: PanelLoadRuntime,
    pending_refresh_status: Option<String>,
}

impl LoadScheduler {
    pub fn prepare_request(&mut self, mut request: NavigationRequest) -> NavigationRequest {
        self.next_generation += 1;
        let generation = self.next_generation;
        panel_runtime_mut(self, request.panel).in_flight_generation = Some(generation);
        request.generation = generation;
        request
    }

    pub fn commit_loaded(&mut self, panel: ActivePanel, generation: u64) -> bool {
        let panel_runtime = panel_runtime_mut(self, panel);
        if panel_runtime.in_flight_generation != Some(generation) {
            return false;
        }
        panel_runtime.in_flight_generation = None;
        panel_runtime.visible_generation = generation;
        true
    }

    pub fn finish_in_flight(&mut self, panel: ActivePanel, generation: u64) {
        let panel_runtime = panel_runtime_mut(self, panel);
        if panel_runtime.in_flight_generation == Some(generation) {
            panel_runtime.in_flight_generation = None;
        }
    }

    pub fn queue_refresh(&mut self, panels: &[ActivePanel], status: String) {
        if panels.is_empty() {
            return;
        }

        self.pending_refresh_status = Some(status);
        for panel in panels {
            panel_runtime_mut(self, *panel).dirty = true;
        }
    }

    pub fn take_next_refresh(&mut self, default_status: &str) -> Option<(ActivePanel, String)> {
        for panel in [ActivePanel::Left, ActivePanel::Right] {
            let panel_runtime = panel_runtime_mut(self, panel);
            if panel_runtime.dirty && panel_runtime.in_flight_generation.is_none() {
                panel_runtime.dirty = false;
                return Some((
                    panel,
                    self.pending_refresh_status
                        .clone()
                        .unwrap_or_else(|| default_status.to_string()),
                ));
            }
        }

        self.pending_refresh_status = None;
        None
    }
}

fn panel_runtime_mut(scheduler: &mut LoadScheduler, panel: ActivePanel) -> &mut PanelLoadRuntime {
    match panel {
        ActivePanel::Left => &mut scheduler.left,
        ActivePanel::Right => &mut scheduler.right,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/application_load_scheduler_tests.rs"]
mod tests;
