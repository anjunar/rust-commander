use std::rc::Rc;

#[cfg(not(target_os = "windows"))]
use std::path::PathBuf;

use crate::ui::main_window::MainWindow;

#[cfg(not(target_os = "windows"))]
use super::context_menu::ContextMenuActions;
use super::{
    context_menu::ContextMenuController,
    hosts::{NavigationHost, OperationsHost, TerminalHost, ViewHost},
    navigation_controller::{NavigationController, NavigationControllerDeps},
    operations_controller::{OperationsController, OperationsControllerDeps},
    terminal_wiring::TerminalController,
    window_state_controller::WindowStateController,
};

#[derive(Clone)]
pub struct MainWindowControllers {
    pub navigation: NavigationController,
    pub operations: OperationsController,
    pub context_menu: ContextMenuController,
    pub terminal: TerminalController,
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
            Rc::clone(&window.commander),
            window.platform_port.clone(),
            #[cfg(not(target_os = "windows"))]
            context_menu_actions(window),
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

        let terminal_host: Rc<dyn TerminalHost> = window.clone();
        let terminal = TerminalController::new(
            terminal_host,
            window.view.terminal.clone(),
            window.view.content.clone(),
        );

        let window_state = WindowStateController::new(
            window.view.root.clone(),
            window.view.panes.clone(),
            window.view.content.clone(),
            Rc::clone(&window.commander),
            window.config_store.clone(),
        );

        Self {
            navigation,
            operations,
            context_menu,
            terminal,
            window_state,
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn context_menu_actions(window: &Rc<MainWindow>) -> ContextMenuActions {
    let action = |run: fn(&Rc<MainWindow>)| {
        let window = Rc::clone(window);
        Rc::new(move || run(&window)) as Rc<dyn Fn()>
    };
    let path_action = |run: fn(&Rc<MainWindow>, Vec<PathBuf>)| {
        let window = Rc::clone(window);
        Rc::new(move |paths| run(&window, paths)) as Rc<dyn Fn(Vec<PathBuf>)>
    };

    ContextMenuActions {
        open: action(MainWindow::handle_open_active),
        rename: action(MainWindow::handle_rename),
        copy: action(MainWindow::handle_copy),
        move_entry: action(MainWindow::handle_move),
        delete: action(MainWindow::handle_delete),
        mkdir: action(MainWindow::handle_make_directory),
        chmod: path_action(MainWindow::handle_unix_chmod),
        chown: path_action(MainWindow::handle_unix_chown),
    }
}
