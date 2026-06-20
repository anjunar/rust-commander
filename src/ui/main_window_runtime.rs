use std::{cell::RefCell, rc::Rc, sync::mpsc::Receiver};

use crate::{
    application::{Commander, ConfigStore, SessionStore, SharedPlatformPort, TaskSpawner},
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
    pub task_spawner: TaskSpawner,
    pub operation_runtime: OperationRuntime,
    pub navigation_runtime: NavigationRuntime,
    pub context_menu_runtime: ContextMenuRuntime,
    pub config_store: ConfigStore,
    pub app_config_cache: Rc<RefCell<AppConfig>>,
    pub platform_port: SharedPlatformPort,
    pub watch_event_rx: Receiver<WatchEvent>,
}

impl MainWindowRuntime {
    pub fn new(
        commander: Commander,
        app_config: AppConfig,
        platform_port: SharedPlatformPort,
    ) -> Self {
        let (watch_command_tx, watch_event_rx) = start_file_watcher();
        let task_spawner = TaskSpawner::default();
        let config_store = ConfigStore::new(app_config);
        let app_config_cache = config_store.cache();

        Self {
            commander: Rc::new(RefCell::new(commander)),
            archive_service: Rc::new(RefCell::new(ArchiveService::with_default_backends(
                task_spawner.clone(),
            ))),
            remote_service: RemoteService::new(task_spawner.clone()),
            session_store: Rc::new(RefCell::new(SessionStore::default())),
            task_spawner,
            operation_runtime: OperationRuntime::new(),
            navigation_runtime: NavigationRuntime::new(watch_command_tx),
            context_menu_runtime: ContextMenuRuntime::new(),
            config_store,
            app_config_cache,
            platform_port,
            watch_event_rx,
        }
    }
}
