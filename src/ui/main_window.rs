use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use gtk::{glib, prelude::*};
use rust_i18n::t;

use crate::{
    application::{
        ActivePanel, AppState, Commander, ConfigStore, SessionStore, SharedPlatformPort,
        TaskSpawner, ViewUpdate,
    },
    archive::ArchiveService,
    config::{AppConfig, WindowConfig},
    platform::assets::asset_path,
    presentation,
    remote::RemoteService,
    ui::{
        dialogs,
        file_clipboard::FileClipboard,
        shell::{build_shell, default_local_directory, ShellEvents, ShellModel, ShellView},
        terminal_controller::TerminalController as TerminalBackend,
        theme::ThemeController,
    },
};

#[path = "main_window_actions.rs"]
mod actions;
#[path = "main_window_actions_clipboard.rs"]
mod actions_clipboard;
#[path = "main_window_context_menu.rs"]
mod context_menu;
#[path = "main_window_controllers.rs"]
mod controllers;
#[path = "main_window_events.rs"]
mod events;
#[path = "main_window_hosts.rs"]
mod hosts;
#[path = "main_window_hosts_impl.rs"]
mod hosts_impl;
#[path = "main_window_navigation.rs"]
mod navigation_controller;
#[path = "main_window_operations.rs"]
mod operations_controller;
#[path = "main_window_runtime.rs"]
mod runtime;
#[path = "main_window_startup.rs"]
mod startup;
#[path = "main_window_terminal.rs"]
mod terminal_wiring;
#[path = "main_window_window_state.rs"]
mod window_state_controller;

use crate::ui::shell::APP_WINDOW_TITLE;

use context_menu::ContextMenuController;
use controllers::MainWindowControllers;
use events::WindowEvents;
use navigation_controller::{NavigationController, NavigationRuntime};
use operations_controller::{OperationRuntime, OperationsController};
pub(crate) use runtime::MainWindowRuntime;
use startup::StartupLoadState;
use terminal_wiring::TerminalController;
use window_state_controller::WindowStateController;

pub struct MainWindow {
    pub window: gtk::ApplicationWindow,
    view: ShellView,
    shell: ShellModel,
    commander: Rc<RefCell<Commander>>,
    archive_service: Rc<RefCell<ArchiveService>>,
    remote_service: RemoteService,
    session_store: Rc<RefCell<SessionStore>>,
    task_spawner: TaskSpawner,
    operation_runtime: OperationRuntime,
    navigation_runtime: NavigationRuntime,
    startup_load_state: Rc<RefCell<StartupLoadState>>,
    initial_window_config: WindowConfig,
    window_state_initialized: Cell<bool>,
    config_store: ConfigStore,
    app_config_cache: Rc<RefCell<AppConfig>>,
    platform_port: SharedPlatformPort,
    controllers: RefCell<Option<MainWindowControllers>>,
    /// While set, `focus_active_panel` does nothing: a panel refresh must not
    /// take the focus from an editor that is just opening.
    suppress_panel_focus: Cell<bool>,
    /// The application's own file clipboard for Ctrl+C, Ctrl+X and Ctrl+V.
    file_clipboard: RefCell<FileClipboard>,
}

impl MainWindow {
    pub fn new_hidden(app: &gtk::Application, runtime: MainWindowRuntime) -> Rc<Self> {
        Self::new_with_visibility(app, runtime, false)
    }

    fn new_with_visibility(
        app: &gtk::Application,
        runtime: MainWindowRuntime,
        present_immediately: bool,
    ) -> Rc<Self> {
        let app_config = runtime.app_config_cache.borrow().clone();
        let window_config = app_config.window.clone();

        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title(APP_WINDOW_TITLE)
            .default_width(window_config.width)
            .default_height(window_config.height)
            .build();

        let asset_icon_dir = asset_path("assets/icons");
        if asset_icon_dir.exists() {
            if let Some(dir_str) = asset_icon_dir.to_str() {
                let icon_theme = gtk::IconTheme::default();
                icon_theme.add_search_path(dir_str);
                window.set_icon_name(Some("rust-commander"));
            }
        }

        let initial_dir = runtime
            .commander
            .borrow()
            .state()
            .active_panel()
            .location
            .host_directory()
            .unwrap_or_else(default_local_directory);
        let terminal = Rc::new(TerminalBackend::new(initial_dir));
        let shell = ShellModel::new(app_config.general.theme);

        let MainWindowRuntime {
            commander,
            archive_service,
            remote_service,
            session_store,
            task_spawner,
            operation_runtime,
            navigation_runtime,
            config_store,
            app_config_cache,
            platform_port,
            watch_event_rx,
        } = runtime;

        // The shell reports to the window through a weak reference: the
        // widgets never keep the window alive, and events arriving before
        // the controllers exist are dropped.
        let this = Rc::new_cyclic(|weak| {
            let events: Rc<dyn ShellEvents> = Rc::new(WindowEvents::new(weak.clone()));
            let view = build_shell(
                &window,
                &shell,
                terminal,
                Rc::new(ThemeController::new()),
                events,
            );
            Self {
                window: window.clone(),
                view,
                shell,
                commander,
                archive_service,
                remote_service,
                session_store,
                task_spawner,
                operation_runtime,
                navigation_runtime,
                startup_load_state: Rc::new(RefCell::new(StartupLoadState::new(
                    !present_immediately,
                ))),
                initial_window_config: window_config.clone(),
                window_state_initialized: Cell::new(present_immediately),
                config_store,
                app_config_cache,
                platform_port,
                controllers: RefCell::new(None),
                suppress_panel_focus: Cell::new(false),
                file_clipboard: RefCell::new(FileClipboard::default()),
            }
        });

        this.initialize_controllers();

        this.apply_update(ViewUpdate::all());
        this.set_toggle_hidden_files_state(this.show_hidden_files_enabled());
        this.navigation_controller().sync_watched_paths();
        this.navigation_controller()
            .install_watcher_poll(watch_event_rx);
        this.window_state_controller()
            .install_window_state_persistence();

        // Initialize Windows tray icon (no-op on other platforms)
        #[cfg(target_os = "windows")]
        {
            let _ = crate::platform::tray::create_tray_icon();
        }

        if present_immediately {
            this.window_state_controller()
                .restore_window_geometry(window_config);
            this.schedule_split_positions();
            this.window_state_controller()
                .install_window_geometry_tracking();
            this.present_window();
        }

        this.restore_panel_session();
        this.navigation_controller().queue_initial_panel_loads();

        this
    }

    /// Brings back the tabs of the last session, or for older configs the
    /// entry the cursor was on.
    fn restore_panel_session(&self) {
        let panes = {
            let app_config = self.app_config_cache.borrow();
            if !app_config.general.restore_last_session {
                return;
            }
            app_config.panes.clone()
        };

        let mut commander = self.commander.borrow_mut();
        if !panes.left_tabs.is_empty() {
            commander.restore_tabs(ActivePanel::Left, &panes.left_tabs, panes.left_active_tab);
        } else if let Some(name) = panes.left_cursor.as_deref().filter(|name| !name.is_empty()) {
            commander.queue_reveal_name_on_panel(ActivePanel::Left, name);
        }
        if !panes.right_tabs.is_empty() {
            commander.restore_tabs(
                ActivePanel::Right,
                &panes.right_tabs,
                panes.right_active_tab,
            );
        } else if let Some(name) = panes
            .right_cursor
            .as_deref()
            .filter(|name| !name.is_empty())
        {
            commander.queue_reveal_name_on_panel(ActivePanel::Right, name);
        }
    }

    pub fn present_window(self: &Rc<Self>) {
        if !self.window_state_initialized.replace(true) {
            self.window_state_controller()
                .restore_window_geometry(self.initial_window_config.clone());
            self.schedule_split_positions();
            self.window_state_controller()
                .install_window_geometry_tracking();
        }
        self.window.present();
        #[cfg(target_os = "windows")]
        {
            glib::idle_add_local_once(move || {
                if let Err(error) = crate::platform::apply_runtime_window_icon(APP_WINDOW_TITLE) {
                    eprintln!("Could not apply Windows runtime icon: {error}");
                }
            });
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let window = self.window.clone();
            glib::idle_add_local_once(move || {
                if let Err(error) = crate::platform::x11_window_icon::apply_window_icon(&window) {
                    eprintln!("Could not apply X11 window icon: {error}");
                }
            });
        }
    }

    pub fn on_initial_panels_ready(self: &Rc<Self>, callback: Rc<dyn Fn()>) {
        let should_run_now = {
            let mut state = self.startup_load_state.borrow_mut();
            if !state.wait_for_initial_panels || state.is_complete() {
                true
            } else {
                state.on_ready = Some(callback.clone());
                false
            }
        };

        if should_run_now {
            glib::idle_add_local_once(move || {
                callback();
            });
        }
    }

    /// Pushes the parts of the commander state named by `update` into the view.
    fn apply_update(&self, update: ViewUpdate) {
        let commander = self.commander.borrow();
        let state = commander.state();

        if update.roots {
            self.apply_root(state, ActivePanel::Left);
            self.apply_root(state, ActivePanel::Right);
        }
        if update.left_entries {
            self.apply_entries(state, ActivePanel::Left);
        }
        if update.right_entries {
            self.apply_entries(state, ActivePanel::Right);
        }
        if update.active_panel {
            self.view
                .left
                .set_active(state.active_panel == ActivePanel::Left);
            self.view
                .right
                .set_active(state.active_panel == ActivePanel::Right);
            self.view.panel(state.active_panel).grab_focus();
        }
        if update.status || update.selection || update.active_panel {
            self.shell.status.set(presentation::status_line(state));
        }

        self.view.terminal.set_panel_dir(
            state
                .active_panel()
                .location
                .host_directory()
                .unwrap_or_else(default_local_directory),
        );
    }

    fn apply_entries(&self, state: &AppState, panel: ActivePanel) {
        let tabs = state.tabs(panel);
        let panel_state = tabs.active();
        let view = self.view.panel(panel);
        view.set_tabs(
            tabs.tabs()
                .iter()
                .map(|tab| tab.panel.location.tab_title())
                .collect(),
            tabs.active_index(),
        );
        view.set_path(&panel_state.location.display_label());
        view.set_footer(&presentation::panel_footer_line(panel_state));
        let size_labels: Vec<String> = panel_state
            .entries
            .iter()
            .map(|entry| panel_state.size_label_for(entry))
            .collect();
        view.set_entries(
            &panel_state.location,
            &panel_state.entries,
            panel_state.cursor_index(),
            panel_state.selection_indices(),
            &size_labels,
        );
        view.apply_sort(
            panel_state.sort_state.column,
            panel_state.sort_state.direction,
        );
    }

    /// Moves the cursor and restyles the marks, without rebuilding the list
    /// or scrolling it.
    pub(crate) fn apply_panel_selection(&self, panel: ActivePanel) {
        let commander = self.commander.borrow();
        let state = commander.state();
        let panel_state = state.panel(panel);
        let view = self.view.panel(panel);
        view.apply_cursor_and_marks(panel_state.cursor_index(), &panel_state.selection_indices());
        view.set_footer(&presentation::panel_footer_line(panel_state));
        self.shell.status.set(presentation::status_line(state));
    }

    /// Brings the view in line after the panel's tabs changed.
    ///
    /// Only the active tabs are listed at startup, and a new tab starts
    /// empty, so the tab that comes to the front may have no entries yet.
    pub(crate) fn show_active_tab(self: &Rc<Self>, panel: ActivePanel, mut update: ViewUpdate) {
        update.active_panel = true;
        let needs_load = self
            .commander
            .borrow()
            .state()
            .panel(panel)
            .entries
            .is_empty();
        self.apply_update(update);
        self.navigation_controller().sync_watched_paths();
        if needs_load {
            let request = crate::application::refresh_request(
                &self.commander.borrow(),
                panel,
                t!("status.ready").into_owned(),
            );
            self.navigation_controller().start_directory_load(request);
        }
    }

    fn apply_root(&self, state: &AppState, panel: ActivePanel) {
        self.view
            .panel(panel)
            .set_roots(&state.roots, state.selected_root_index(panel));
    }

    fn active_panel_path(&self) -> std::path::PathBuf {
        self.commander
            .borrow()
            .state()
            .active_panel()
            .location
            .host_directory()
            .unwrap_or_else(default_local_directory)
    }

    pub(crate) fn focus_active_panel(&self) {
        if self.suppress_panel_focus.get() {
            return;
        }
        let active_panel = self.commander.borrow().state().active_panel;
        self.view.panel(active_panel).grab_focus();
    }

    pub(crate) fn clear_panel_focus_suppress_after(self: &Rc<Self>, duration: std::time::Duration) {
        let this = Rc::clone(self);
        glib::timeout_add_local_once(duration, move || {
            this.suppress_panel_focus.set(false);
        });
    }

    pub(crate) fn refresh_active_panel_revealing(self: &Rc<Self>, name: &str, status: String) {
        let navigation = self.navigation_controller();
        // A rename or create touches the directory; the watcher must not
        // rebuild the panel in the same moment as this intentional refresh.
        navigation.trigger_manual_refresh_cooldown();
        let request = {
            let mut commander = self.commander.borrow_mut();
            let panel = commander.state().active_panel;
            commander.queue_reveal_name_on_active(name);
            commander.set_status(status.clone());
            crate::application::refresh_request(&commander, panel, status)
        };
        navigation.start_directory_load(request);
    }

    fn set_status_message(&self, status: String) {
        let update = {
            let mut commander = self.commander.borrow_mut();
            commander.set_status(status)
        };
        self.apply_update(update);
    }

    fn show_command_failed(&self, error: impl std::fmt::Display) {
        let error = error.to_string();
        self.set_status_message(t!("status.command_failed", error = error.as_str()).into_owned());
        dialogs::show_error(&self.window, &t!("error.command_failed"), &error);
    }

    fn schedule_split_positions(self: &Rc<Self>) {
        let this = Rc::clone(self);
        self.window_state_controller()
            .initialize_split_positions(move || {
                this.view.left.refresh_list_after_resize();
                this.view.right.refresh_list_after_resize();
            });
    }

    /// While a directory loads, only that panel is disabled; the feedback
    /// stays in the status line.
    fn set_navigation_busy(&self, panel: ActivePanel, busy: bool, message: &str) {
        self.navigation_runtime.navigation_busy.set(busy);
        self.shell.busy.set(busy);
        self.view.panel(panel).set_interaction_enabled(!busy);
        let status = if busy {
            message.to_owned()
        } else {
            presentation::status_line(self.commander.borrow().state())
        };
        self.shell.status.set(status);
    }

    fn navigation_controller(self: &Rc<Self>) -> NavigationController {
        self.controllers()
            .expect("controllers initialized")
            .navigation
    }

    fn operations_controller(self: &Rc<Self>) -> OperationsController {
        self.controllers()
            .expect("controllers initialized")
            .operations
    }

    fn context_menu_controller(&self) -> ContextMenuController {
        self.controllers()
            .expect("controllers initialized")
            .context_menu
    }

    fn terminal_controller(self: &Rc<Self>) -> TerminalController {
        self.controllers()
            .expect("controllers initialized")
            .terminal
    }

    fn window_state_controller(&self) -> WindowStateController {
        self.controllers()
            .expect("controllers initialized")
            .window_state
    }

    fn handle_terminal_action(
        self: &Rc<Self>,
        action: crate::ui::terminal_controller::TerminalAction,
    ) {
        self.terminal_controller().handle_terminal_action(action);
    }

    fn initialize_controllers(self: &Rc<Self>) {
        self.controllers
            .replace(Some(MainWindowControllers::build(self)));
    }

    fn controllers(&self) -> Option<MainWindowControllers> {
        self.controllers.borrow().as_ref().cloned()
    }
}
