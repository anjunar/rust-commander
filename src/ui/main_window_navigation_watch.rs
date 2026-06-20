use std::{
    collections::BTreeSet,
    path::PathBuf,
    rc::Rc,
    sync::mpsc::Receiver,
    time::{Duration, Instant},
};

use gtk::glib;
use rust_i18n::t;

use crate::{
    application::{refresh_request, ActivePanel},
    fs::watcher::WatchEvent,
};

use super::NavigationController;

impl NavigationController {
    pub fn refresh_dirty_panels_if_idle(&self) {
        if self.operation_runtime.active_operation.borrow().is_some()
            || self.runtime.navigation_busy.get()
            || self.is_watcher_refresh_suppressed()
        {
            return;
        }

        let Some((panel, status)) = self
            .runtime
            .load_scheduler
            .borrow_mut()
            .take_next_refresh(&t!("status.view_refreshed").into_owned())
        else {
            return;
        };
        let request = {
            let commander = self.commander.borrow();
            refresh_request(&commander, panel, status)
        };
        self.start_directory_load(request);
    }

    pub fn queue_initial_panel_loads(&self) {
        self.runtime.load_scheduler.borrow_mut().queue_refresh(
            &[ActivePanel::Left, ActivePanel::Right],
            t!("status.view_refreshed").into_owned(),
        );
        self.refresh_dirty_panels_if_idle();
    }

    pub fn install_watcher_poll(&self, watch_event_rx: Receiver<WatchEvent>) {
        let controller = self.clone();
        let pending_paths = Rc::new(std::cell::RefCell::new(Vec::<PathBuf>::new()));
        glib::timeout_add_local(Duration::from_millis(350), move || {
            let mut drained_paths = Vec::new();
            while let Ok(event) = watch_event_rx.try_recv() {
                drained_paths.extend(event.paths);
            }

            if !drained_paths.is_empty() {
                let mut pending = pending_paths.borrow_mut();
                pending.extend(drained_paths);
            }

            if controller
                .operation_runtime
                .active_operation
                .borrow()
                .is_some()
                || controller.runtime.navigation_busy.get()
            {
                return glib::ControlFlow::Continue;
            }

            let changed_paths = {
                let mut pending = pending_paths.borrow_mut();
                if pending.is_empty() {
                    return glib::ControlFlow::Continue;
                }
                std::mem::take(&mut *pending)
            };

            controller.apply_watcher_changes(&changed_paths);

            glib::ControlFlow::Continue
        });
    }

    pub fn affected_panels_for_paths(&self, changed_paths: &[PathBuf]) -> Vec<ActivePanel> {
        let commander = self.commander.borrow();
        let state = commander.state();
        let mut affected = Vec::new();

        for panel in [ActivePanel::Left, ActivePanel::Right] {
            let Some(panel_path) = state.panel(panel).location.filesystem_path() else {
                continue;
            };
            if changed_paths
                .iter()
                .any(|path| path == panel_path || path.parent() == Some(panel_path))
            {
                affected.push(panel);
            }
        }

        affected
    }

    fn apply_watcher_changes(&self, changed_paths: &[PathBuf]) {
        let deduped_paths = dedupe_paths(changed_paths);
        if deduped_paths.is_empty() {
            return;
        }

        let show_hidden_files = self.app_config_cache.borrow().panels.show_hidden_files;
        for panel in self.affected_panels_for_paths(&deduped_paths) {
            let relevant_paths = deduped_paths
                .iter()
                .filter(|path| self.path_affects_panel(panel, path))
                .cloned()
                .collect::<Vec<_>>();
            if relevant_paths.is_empty() {
                continue;
            }

            let update = {
                let mut commander = self.commander.borrow_mut();
                match commander.apply_filesystem_entry_changes(
                    panel,
                    &relevant_paths,
                    show_hidden_files,
                ) {
                    Ok(update) => update,
                    Err(error) => {
                        drop(commander);
                        self.show_command_failed(error);
                        return;
                    }
                }
            };

            if let Some(update) = update {
                self.host.apply_update(update);
            }
        }
    }

    pub(super) fn trigger_manual_refresh_cooldown(&self) {
        self.runtime
            .watcher_refresh_cooldown_until
            .set(Some(Instant::now() + Duration::from_millis(900)));
        self.sync_watched_paths();
    }

    fn is_watcher_refresh_suppressed(&self) -> bool {
        match self.runtime.watcher_refresh_cooldown_until.get() {
            Some(until) if Instant::now() < until => true,
            Some(_) => {
                self.runtime.watcher_refresh_cooldown_until.set(None);
                false
            }
            None => false,
        }
    }

    fn path_affects_panel(&self, panel: ActivePanel, path: &PathBuf) -> bool {
        let commander = self.commander.borrow();
        let Some(panel_path) = commander.state().panel(panel).location.filesystem_path() else {
            return false;
        };
        path.parent() == Some(panel_path)
    }
}

fn dedupe_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen = BTreeSet::new();
    let mut deduped = Vec::new();

    for path in paths {
        if seen.insert(path.clone()) {
            deduped.push(path.clone());
        }
    }

    deduped
}
