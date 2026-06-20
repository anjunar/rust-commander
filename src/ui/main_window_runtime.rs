use std::{cell::RefCell, rc::Rc, sync::mpsc::Receiver};

use crate::{
    application::{Commander, SessionStore},
    archive::ArchiveService,
    config::AppConfig,
    fs::watcher::{start_file_watcher, WatchEvent},
    remote::RemoteService,
};

use super::{
    context_menu::ContextMenuRuntime, navigation_controller::NavigationRuntime,
    operations_controller::OperationRuntime,
};

pub struct MainWindowRuntime {
    pub commander: Rc<RefCell<Commander>>,
    pub archive_service: Rc<RefCell<ArchiveService>>,
    pub remote_service: RemoteService,
    pub session_store: Rc<RefCell<SessionStore>>,
    pub operation_runtime: OperationRuntime,
    pub navigation_runtime: NavigationRuntime,
    pub context_menu_runtime: ContextMenuRuntime,
    pub app_config_cache: Rc<RefCell<AppConfig>>,
    pub watch_event_rx: Receiver<WatchEvent>,
}

impl MainWindowRuntime {
    pub fn new(commander: Commander, app_config: AppConfig) -> Self {
        let (watch_command_tx, watch_event_rx) = start_file_watcher();

        Self {
            commander: Rc::new(RefCell::new(commander)),
            archive_service: Rc::new(RefCell::new(ArchiveService::with_default_backends())),
            remote_service: RemoteService::default(),
            session_store: Rc::new(RefCell::new(SessionStore::default())),
            operation_runtime: OperationRuntime::new(),
            navigation_runtime: NavigationRuntime::new(watch_command_tx),
            context_menu_runtime: ContextMenuRuntime::new(),
            app_config_cache: Rc::new(RefCell::new(app_config)),
            watch_event_rx,
        }
    }
}
