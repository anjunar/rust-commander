use std::rc::Rc;

#[cfg(not(target_os = "windows"))]
use std::path::PathBuf;

use crate::ui::main_window::MainWindow;

#[cfg(not(target_os = "windows"))]
use super::context_menu::UnixContextMenuActions;
use super::{
    context_menu::ContextMenuController,
    hosts::{NavigationHost, OperationsHost, TerminalHost, ViewHost},
    navigation_controller::{NavigationController, NavigationControllerDeps},
    operations_controller::{OperationsController, OperationsControllerDeps},
    panel_wiring::PanelWiring,
    terminal_wiring::TerminalController,
    window_chrome::WindowChromeController,
    window_state_controller::WindowStateController,
};

#[derive(Clone)]
pub struct MainWindowControllers {
    pub navigation: NavigationController,
    pub operations: OperationsController,
    pub panel_wiring: PanelWiring,
    pub terminal: TerminalController,
    pub window_chrome: WindowChromeController,
    pub window_state: WindowStateController,
}

impl MainWindowControllers {
    pub fn build(window: &Rc<MainWindow>) -> Self {
        let navigation_host: Rc<dyn NavigationHost> = window.clone();
        let navigation = NavigationController::new(NavigationControllerDeps {
            host: navigation_host,
            window: window.window.clone(),
            commander: Rc::clone(&window.commander),
            archive_service: Rc::clone(&window.archive_service),
            remote_service: window.remote_service.clone(),
            session_store: Rc::clone(&window.session_store),
            task_spawner: window.task_spawner.clone(),
            operation_runtime: window.operation_runtime.clone(),
            runtime: window.navigation_runtime.clone(),
            app_config_cache: Rc::clone(&window.app_config_cache),
            platform_port: window.platform_port.clone(),
        });

        let context_menu_host: Rc<dyn ViewHost> = window.clone();
        let context_menu = ContextMenuController::new(
            context_menu_host,
            window.window.clone(),
            #[cfg(not(target_os = "windows"))]
            window.commander_view.left.root.clone(),
            #[cfg(not(target_os = "windows"))]
            window.commander_view.right.root.clone(),
            Rc::clone(&window.commander),
            window.platform_port.clone(),
            window.context_menu_runtime.clone(),
            #[cfg(not(target_os = "windows"))]
            unix_context_menu_actions(window),
        );

        let operations_host: Rc<dyn OperationsHost> = window.clone();
        let operations = OperationsController::new(OperationsControllerDeps {
            host: operations_host,
            window: window.window.clone(),
            commander: Rc::clone(&window.commander),
            archive_service: Rc::clone(&window.archive_service),
            remote_service: window.remote_service.clone(),
            session_store: Rc::clone(&window.session_store),
            task_spawner: window.task_spawner.clone(),
            runtime: window.operation_runtime.clone(),
            app_config_cache: Rc::clone(&window.app_config_cache),
            navigation: navigation.clone(),
        });

        let panel_wiring_host: Rc<dyn ViewHost> = window.clone();
        let context_menu_controller = context_menu.clone();
        let context_menu_handler = Rc::new(move |panel, clicked_index, x, y| {
            context_menu_controller.handle_panel_context_menu(panel, clicked_index, x, y);
        });
        let remote_window = Rc::clone(window);
        let remote_connect_handler = Rc::new(move |panel| {
            remote_window.handle_connect_remote_for_panel(panel);
        });
        let panel_wiring = PanelWiring::new(
            panel_wiring_host,
            Rc::clone(&window.commander),
            navigation.clone(),
            context_menu_handler,
            remote_connect_handler,
        );

        let terminal_host: Rc<dyn TerminalHost> = window.clone();
        let terminal = TerminalController::new(
            terminal_host,
            window.terminal_dock.clone(),
            window.content_paned.clone(),
        );

        let window_chrome = WindowChromeController::new(
            window.window.clone(),
            Rc::clone(&window.app_config_cache),
            Rc::clone(&window.theme_controller),
        );

        let window_state = WindowStateController::new(
            window.window.clone(),
            window.commander_view.root.clone(),
            window.content_paned.clone(),
            Rc::clone(&window.commander),
            window.config_store.clone(),
        );

        Self {
            navigation,
            operations,
            panel_wiring,
            terminal,
            window_chrome,
            window_state,
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn unix_context_menu_actions(window: &Rc<MainWindow>) -> UnixContextMenuActions {
    let open = {
        let window = Rc::clone(window);
        Rc::new(move || window.handle_open_active()) as Rc<dyn Fn()>
    };
    let rename = {
        let window = Rc::clone(window);
        Rc::new(move || window.handle_rename()) as Rc<dyn Fn()>
    };
    let copy = {
        let window = Rc::clone(window);
        Rc::new(move || window.handle_copy()) as Rc<dyn Fn()>
    };
    let move_entry = {
        let window = Rc::clone(window);
        Rc::new(move || window.handle_move()) as Rc<dyn Fn()>
    };
    let delete = {
        let window = Rc::clone(window);
        Rc::new(move || window.handle_delete()) as Rc<dyn Fn()>
    };
    let mkdir = {
        let window = Rc::clone(window);
        Rc::new(move || window.handle_make_directory()) as Rc<dyn Fn()>
    };
    let chmod = {
        let window = Rc::clone(window);
        Rc::new(move |paths| window.handle_unix_chmod(paths)) as Rc<dyn Fn(Vec<PathBuf>)>
    };
    let chown = {
        let window = Rc::clone(window);
        Rc::new(move |paths| window.handle_unix_chown(paths)) as Rc<dyn Fn(Vec<PathBuf>)>
    };

    UnixContextMenuActions {
        open,
        rename,
        copy,
        move_entry,
        delete,
        mkdir,
        chmod,
        chown,
    }
}
