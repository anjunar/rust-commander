use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use gtk::{glib, prelude::*};

use crate::{
    application::{
        ActivePanel, Commander, ConfigStore, SessionStore, SharedPlatformPort, TaskSpawner,
        ViewUpdate,
    },
    archive::ArchiveService,
    config::{AppConfig, WindowConfig},
    platform::assets::asset_path,
    presentation,
    remote::RemoteService,
    ui::{
        commander_view::CommanderView, dialogs, shortcuts, terminal_dock::TerminalDock,
        theme::ThemeController,
    },
};

#[path = "main_window_actions.rs"]
mod actions;
#[path = "main_window_command_bar.rs"]
mod command_bar;
#[path = "main_window_context_menu.rs"]
mod context_menu;
#[path = "main_window_controllers.rs"]
mod controllers;
#[path = "main_window_hosts.rs"]
mod hosts;
#[path = "main_window_hosts_impl.rs"]
mod hosts_impl;
#[path = "main_window_navigation.rs"]
mod navigation_controller;
#[path = "main_window_operations.rs"]
mod operations_controller;
#[path = "main_window_panel_wiring.rs"]
mod panel_wiring;
#[path = "main_window_runtime.rs"]
mod runtime;
#[path = "main_window_startup.rs"]
mod startup;
#[path = "main_window_terminal.rs"]
mod terminal_wiring;
#[path = "main_window_window_chrome.rs"]
mod window_chrome;
#[path = "main_window_window_state.rs"]
mod window_state_controller;

const APP_WINDOW_TITLE: &str = "RCommander";

use command_bar::build_command_bar;
use context_menu::ContextMenuRuntime;
use controllers::MainWindowControllers;
use navigation_controller::{NavigationController, NavigationRuntime};
use operations_controller::{OperationRuntime, OperationsController};
use panel_wiring::PanelWiring;
pub(crate) use runtime::MainWindowRuntime;
use startup::StartupLoadState;
use terminal_wiring::TerminalController;
#[cfg(target_os = "windows")]
use window_chrome::install_custom_window_controls;
use window_chrome::WindowChromeController;
use window_state_controller::WindowStateController;

pub struct MainWindow {
    pub window: gtk::ApplicationWindow,
    commander_view: CommanderView,
    terminal_dock: TerminalDock,
    content_paned: gtk::Paned,
    navigation_overlay: gtk::Box,
    navigation_overlay_spinner: gtk::Spinner,
    navigation_overlay_label: gtk::Label,
    busy_spinner: gtk::Spinner,
    status_label: gtk::Label,
    commander: Rc<RefCell<Commander>>,
    archive_service: Rc<RefCell<ArchiveService>>,
    remote_service: RemoteService,
    session_store: Rc<RefCell<SessionStore>>,
    task_spawner: TaskSpawner,
    operation_runtime: OperationRuntime,
    navigation_runtime: NavigationRuntime,
    context_menu_runtime: ContextMenuRuntime,
    startup_load_state: Rc<RefCell<StartupLoadState>>,
    initial_window_config: WindowConfig,
    window_state_initialized: Cell<bool>,
    config_store: ConfigStore,
    app_config_cache: Rc<RefCell<AppConfig>>,
    platform_port: SharedPlatformPort,
    controllers: RefCell<Option<MainWindowControllers>>,
    theme_controller: Rc<ThemeController>,
}

impl MainWindow {
    #[allow(dead_code)]
    pub fn new(app: &gtk::Application, runtime: MainWindowRuntime) -> Rc<Self> {
        Self::new_with_visibility(app, runtime, true)
    }

    pub fn new_hidden(app: &gtk::Application, runtime: MainWindowRuntime) -> Rc<Self> {
        Self::new_with_visibility(app, runtime, false)
    }

    fn new_with_visibility(
        app: &gtk::Application,
        runtime: MainWindowRuntime,
        present_immediately: bool,
    ) -> Rc<Self> {
        let app_config = runtime.app_config_cache.borrow().clone();
        let theme_controller = Rc::new(ThemeController::new());
        theme_controller.apply(app_config.general.theme);
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

        #[cfg(not(target_os = "macos"))]
        {
            let header = gtk::HeaderBar::new();
            let title = gtk::Label::new(Some(APP_WINDOW_TITLE));
            title.add_css_class("app-title");
            header.set_title_widget(Some(&title));
            #[cfg(target_os = "windows")]
            install_custom_window_controls(&window, &header);
            window.set_titlebar(Some(&header));
        }

        let shell = gtk::Box::new(gtk::Orientation::Vertical, 8);
        shell.set_margin_top(8);
        shell.set_margin_bottom(8);
        shell.set_margin_start(8);
        shell.set_margin_end(8);

        let commander_view = CommanderView::new();
        let initial_dir = runtime
            .commander
            .borrow()
            .state()
            .active_panel()
            .location
            .host_directory()
            .unwrap_or_else(default_local_directory);
        let terminal_dock = TerminalDock::new(initial_dir);

        let content_paned = gtk::Paned::new(gtk::Orientation::Vertical);
        content_paned.set_hexpand(true);
        content_paned.set_vexpand(true);
        content_paned.set_resize_start_child(true);
        content_paned.set_resize_end_child(false);
        content_paned.set_shrink_start_child(false);
        content_paned.set_shrink_end_child(false);
        content_paned.set_start_child(Some(&commander_view.root));
        content_paned.set_end_child(Some(&terminal_dock.root));

        let content_overlay = gtk::Overlay::new();
        content_overlay.set_hexpand(true);
        content_overlay.set_vexpand(true);
        content_overlay.set_child(Some(&content_paned));

        let navigation_overlay = gtk::Box::new(gtk::Orientation::Vertical, 12);
        navigation_overlay.set_halign(gtk::Align::Center);
        navigation_overlay.set_valign(gtk::Align::Center);
        navigation_overlay.add_css_class("navigation-overlay");
        navigation_overlay.set_opacity(0.0);
        navigation_overlay.set_can_target(false);
        navigation_overlay.set_sensitive(false);

        let navigation_overlay_spinner = gtk::Spinner::new();
        navigation_overlay_spinner.stop();
        navigation_overlay_spinner.set_visible(true);
        navigation_overlay_spinner.add_css_class("navigation-overlay-spinner");
        navigation_overlay.append(&navigation_overlay_spinner);

        let navigation_overlay_label = gtk::Label::new(Some(""));
        navigation_overlay_label.add_css_class("navigation-overlay-label");
        navigation_overlay.append(&navigation_overlay_label);

        content_overlay.add_overlay(&navigation_overlay);
        shell.append(&content_overlay);

        let status_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let busy_spinner = gtk::Spinner::new();
        busy_spinner.set_spinning(false);
        busy_spinner.set_visible(false);
        busy_spinner.add_css_class("busy-spinner");
        status_row.append(&busy_spinner);

        let status_label = gtk::Label::new(None);
        status_label.set_xalign(0.0);
        status_label.set_hexpand(true);
        status_label.add_css_class("status-line");
        status_row.append(&status_label);
        shell.append(&status_row);

        let command_bar = build_command_bar();
        shell.append(&command_bar);

        window.set_child(Some(&shell));

        let startup_load_state = Rc::new(RefCell::new(StartupLoadState::new(!present_immediately)));
        let MainWindowRuntime {
            commander,
            archive_service,
            remote_service,
            session_store,
            task_spawner,
            operation_runtime,
            navigation_runtime,
            context_menu_runtime,
            config_store,
            app_config_cache,
            platform_port,
            watch_event_rx,
        } = runtime;

        let this = Rc::new(Self {
            window,
            commander_view,
            terminal_dock,
            content_paned,
            navigation_overlay,
            navigation_overlay_spinner,
            navigation_overlay_label,
            busy_spinner,
            status_label,
            commander,
            archive_service,
            remote_service,
            session_store,
            task_spawner,
            operation_runtime,
            navigation_runtime,
            context_menu_runtime,
            startup_load_state,
            initial_window_config: window_config.clone(),
            window_state_initialized: Cell::new(present_immediately),
            config_store,
            app_config_cache,
            platform_port,
            controllers: RefCell::new(None),
            theme_controller,
        });

        this.initialize_controllers();

        this.apply_update(ViewUpdate::all());
        this.window_chrome().apply_theme();
        this.window_chrome()
            .refresh_localized_labels(&this.commander_view, &this.terminal_dock);
        this.navigation_controller().sync_watched_paths();
        this.panel_wiring().connect_panels(&this.commander_view);
        this.connect_command_bar(&command_bar);
        this.terminal_controller().connect_terminal_dock();
        this.navigation_controller()
            .install_watcher_poll(watch_event_rx);
        this.window_state_controller()
            .install_window_state_persistence();
        this.window_chrome().install_system_theme_tracking();
        shortcuts::install(&this, app);

        // Initialize Windows tray icon (no-op on other platforms)
        #[cfg(target_os = "windows")]
        {
            let _ = crate::platform::tray::create_tray_icon();
        }

        if present_immediately {
            this.window_state_controller()
                .restore_window_geometry(window_config);
            this.window_state_controller().initialize_split_positions();
            this.window_state_controller()
                .install_window_geometry_tracking();
            this.present_window();
        }
        this.navigation_controller().queue_initial_panel_loads();

        this
    }

    pub fn present_window(self: &Rc<Self>) {
        if !self.window_state_initialized.replace(true) {
            self.window_state_controller()
                .restore_window_geometry(self.initial_window_config.clone());
            self.window_state_controller().initialize_split_positions();
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

    fn connect_command_bar(self: &Rc<Self>, command_bar: &gtk::Box) {
        let callbacks: [fn(&Rc<Self>); 10] = [
            Self::handle_help,
            Self::handle_rename,
            Self::handle_view,
            Self::handle_edit,
            Self::handle_copy,
            Self::handle_move,
            Self::handle_make_directory,
            Self::handle_delete,
            Self::handle_toggle_terminal,
            Self::handle_quit,
        ];

        let mut callback_index = 0usize;
        let mut child = command_bar.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if let Ok(button) = widget.downcast::<gtk::Button>() {
                if let Some(callback) = callbacks.get(callback_index) {
                    let this = Rc::clone(self);
                    let callback = *callback;
                    button.connect_clicked(move |_| callback(&this));
                }
                callback_index += 1;
            }
        }
    }

    fn apply_update(&self, update: ViewUpdate) {
        let commander = self.commander.borrow();
        let state = commander.state();

        if update.roots {
            self.commander_view.apply_roots(state);
        }
        if update.left_entries {
            self.commander_view.apply_entries(state, ActivePanel::Left);
        }
        if update.right_entries {
            self.commander_view.apply_entries(state, ActivePanel::Right);
        }
        if update.active_panel {
            self.commander_view.apply_active_panel(state.active_panel);
        }
        if update.status || update.selection || update.active_panel {
            self.status_label
                .set_label(&presentation::status_line(state));
        }

        self.terminal_dock.set_panel_dir(
            state
                .active_panel()
                .location
                .host_directory()
                .unwrap_or_else(default_local_directory),
        );
        self.terminal_dock.refresh_toolbar();
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

    fn focus_active_panel(&self) {
        let active_panel = self.commander.borrow().state().active_panel;
        self.commander_view.focus_active_panel(active_panel);
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

    fn set_navigation_busy(&self, busy: bool, message: &str) {
        self.navigation_runtime.navigation_busy.set(busy);
        self.busy_spinner.set_visible(busy);
        self.busy_spinner.set_spinning(busy);
        self.navigation_overlay
            .set_opacity(if busy { 1.0 } else { 0.0 });
        self.navigation_overlay.set_sensitive(busy);
        if busy {
            self.navigation_overlay_spinner.start();
        } else {
            self.navigation_overlay_spinner.stop();
        }
        self.navigation_overlay_label.set_label(message);
        self.commander_view.set_interaction_enabled(!busy);
        self.content_paned.set_sensitive(!busy);

        if busy {
            self.status_label.set_label(message);
        } else {
            self.status_label
                .set_label(&presentation::status_line(self.commander.borrow().state()));
        }
    }

    fn navigation_controller(self: &Rc<Self>) -> NavigationController {
        self.controllers()
            .expect("controllers initialized")
            .navigation
    }

    fn panel_wiring(self: &Rc<Self>) -> PanelWiring {
        self.controllers()
            .expect("controllers initialized")
            .panel_wiring
    }

    fn operations_controller(self: &Rc<Self>) -> OperationsController {
        self.controllers()
            .expect("controllers initialized")
            .operations
    }

    fn terminal_controller(self: &Rc<Self>) -> TerminalController {
        self.controllers()
            .expect("controllers initialized")
            .terminal
    }

    fn window_chrome(&self) -> WindowChromeController {
        self.controllers()
            .expect("controllers initialized")
            .window_chrome
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

fn default_local_directory() -> std::path::PathBuf {
    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
}
