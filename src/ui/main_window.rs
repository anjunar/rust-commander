use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use gtk::{gdk, glib, prelude::*};

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
        commander_view::CommanderView, dialogs, file_clipboard::FileClipboard, shortcuts,
        terminal_dock::TerminalDock, theme::ThemeController,
    },
};

#[path = "main_window_actions.rs"]
mod actions;
#[path = "main_window_actions_clipboard.rs"]
mod actions_clipboard;
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
#[path = "main_window_menu.rs"]
mod menu;
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

use command_bar::{apply_command_bar_labels, build_command_bar, CommandBarLabelMode};
use context_menu::ContextMenuRuntime;
use controllers::MainWindowControllers;
use menu::build_menu_bar;
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
    busy_spinner: gtk::Spinner,
    status_label: gtk::Label,
    command_bar: gtk::Box,
    command_bar_label_mode: Cell<CommandBarLabelMode>,
    menu_bar: Option<gtk::PopoverMenuBar>,
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
    /// When set, `focus_active_panel` is a no-op (e.g. while opening the editor
    /// after creating a file — panel refresh must not steal OS focus).
    suppress_panel_focus: Cell<bool>,
    /// App-owned file clipboard for Ctrl+C / Ctrl+X / Ctrl+V (cut/copy/paste).
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
        let menu_bar = {
            let header = gtk::HeaderBar::new();
            let title = gtk::Label::new(Some(APP_WINDOW_TITLE));
            title.add_css_class("app-title");
            header.set_title_widget(Some(&title));
            let menu_bar = build_menu_bar();
            header.pack_start(&menu_bar);
            #[cfg(target_os = "windows")]
            install_custom_window_controls(&window, &header);
            window.set_titlebar(Some(&header));
            Some(menu_bar)
        };
        #[cfg(target_os = "macos")]
        let menu_bar = None;

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
        shell.append(&content_paned);

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
            busy_spinner,
            status_label,
            command_bar: command_bar.clone(),
            command_bar_label_mode: Cell::new(CommandBarLabelMode::Normal),
            menu_bar,
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
            suppress_panel_focus: Cell::new(false),
            file_clipboard: RefCell::new(FileClipboard::default()),
        });

        this.initialize_controllers();

        this.apply_update(ViewUpdate::all());
        this.window_chrome().apply_theme();
        this.window_chrome().refresh_localized_labels(
            &this.commander_view,
            &this.terminal_dock,
            this.menu_bar.as_ref(),
        );
        this.refresh_command_bar_labels();
        this.navigation_controller().sync_watched_paths();
        this.panel_wiring().connect_panels(&this.commander_view);
        this.connect_command_bar(&command_bar);
        this.install_alt_command_mode_tracking();
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
            this.schedule_split_positions();
            this.window_state_controller()
                .install_window_geometry_tracking();
            this.present_window();
        }

        {
            let app_config = this.app_config_cache.borrow();
            if app_config.general.restore_last_session {
                let left_cursor = app_config.panes.left_cursor.clone();
                let right_cursor = app_config.panes.right_cursor.clone();
                let left_tabs = app_config.panes.left_tabs.clone();
                let right_tabs = app_config.panes.right_tabs.clone();
                let left_active = app_config.panes.left_active_tab;
                let right_active = app_config.panes.right_active_tab;
                drop(app_config);
                let mut commander = this.commander.borrow_mut();
                if !left_tabs.is_empty() {
                    commander.restore_tabs(ActivePanel::Left, &left_tabs, left_active);
                } else if let Some(name) = left_cursor.as_deref().filter(|name| !name.is_empty()) {
                    commander.queue_reveal_name_on_panel(ActivePanel::Left, name);
                }
                if !right_tabs.is_empty() {
                    commander.restore_tabs(ActivePanel::Right, &right_tabs, right_active);
                } else if let Some(name) = right_cursor.as_deref().filter(|name| !name.is_empty()) {
                    commander.queue_reveal_name_on_panel(ActivePanel::Right, name);
                }
            }
        }

        this.navigation_controller().queue_initial_panel_loads();

        this
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
        let alt_callbacks: [Option<fn(&Rc<Self>)>; 10] = [
            Some(Self::handle_select_root_left),
            Some(Self::handle_select_root_right),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ];
        let ctrl_callbacks: [Option<fn(&Rc<Self>)>; 10] = [
            None,
            None,
            Some(Self::handle_sort_by_name),
            Some(Self::handle_sort_by_extension),
            Some(Self::handle_sort_by_date),
            Some(Self::handle_sort_by_size),
            None,
            None,
            None,
            None,
        ];
        let shift_callbacks: [Option<fn(&Rc<Self>)>; 10] = [
            None,
            None,
            None,
            Some(Self::handle_create_text_file),
            Some(Self::handle_copy_with_rename),
            None,
            None,
            Some(Self::handle_delete_permanent),
            None,
            None,
        ];

        let mut callback_index = 0usize;
        let mut child = command_bar.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if let Ok(button) = widget.downcast::<gtk::Button>() {
                if let Some(callback) = callbacks.get(callback_index) {
                    let this = Rc::clone(self);
                    let callback = *callback;
                    let alt_callback = alt_callbacks[callback_index];
                    let ctrl_callback = ctrl_callbacks[callback_index];
                    let shift_callback = shift_callbacks[callback_index];
                    button.connect_clicked(move |_| {
                        match this.command_bar_label_mode.get() {
                            CommandBarLabelMode::Alt => {
                                if let Some(alt_callback) = alt_callback {
                                    alt_callback(&this);
                                    return;
                                }
                            }
                            CommandBarLabelMode::Ctrl => {
                                if let Some(ctrl_callback) = ctrl_callback {
                                    ctrl_callback(&this);
                                    return;
                                }
                            }
                            CommandBarLabelMode::Shift => {
                                if let Some(shift_callback) = shift_callback {
                                    shift_callback(&this);
                                    return;
                                }
                            }
                            CommandBarLabelMode::Normal | CommandBarLabelMode::Hidden => {}
                        }
                        callback(&this);
                    });
                }
                callback_index += 1;
            }
        }
    }

    fn install_alt_command_mode_tracking(self: &Rc<Self>) {
        let key_controller = gtk::EventControllerKey::new();
        key_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        {
            let this = Rc::clone(self);
            key_controller.connect_key_pressed(move |_, key, _, modifiers| {
                let modifiers = effective_modifiers(key, modifiers, true);
                this.set_command_bar_label_mode(command_bar_label_mode_from(modifiers));

                // Capture plain F10 before GTK/Windows menu handling can swallow it.
                if key == gdk::Key::F10
                    && !modifiers.intersects(
                        gdk::ModifierType::ALT_MASK
                            | gdk::ModifierType::CONTROL_MASK
                            | gdk::ModifierType::SHIFT_MASK
                            | gdk::ModifierType::SUPER_MASK,
                    )
                {
                    this.handle_quit();
                    return glib::Propagation::Stop;
                }

                glib::Propagation::Proceed
            });
        }
        {
            let this = Rc::clone(self);
            key_controller.connect_key_released(move |_, key, _, modifiers| {
                let modifiers = effective_modifiers(key, modifiers, false);
                this.set_command_bar_label_mode(command_bar_label_mode_from(modifiers));
            });
        }
        self.window.add_controller(key_controller);

        let this = Rc::clone(self);
        self.window.connect_is_active_notify(move |window| {
            if !window.is_active() {
                this.set_command_bar_label_mode(CommandBarLabelMode::Normal);
            }
        });
    }

    fn set_command_bar_label_mode(self: &Rc<Self>, mode: CommandBarLabelMode) {
        if self.command_bar_label_mode.get() == mode {
            return;
        }
        self.command_bar_label_mode.set(mode);
        self.refresh_command_bar_labels();
    }

    pub(crate) fn refresh_command_bar_labels(&self) {
        apply_command_bar_labels(&self.command_bar, self.command_bar_label_mode.get());
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

    pub(crate) fn apply_panel_selection(&self, panel: ActivePanel) {
        let commander = self.commander.borrow();
        let state = commander.state();
        let panel_state = state.panel(panel);
        let panel_view = self.commander_view.panel(panel);
        panel_view
            .apply_cursor_and_marks(panel_state.cursor_index(), &panel_state.selection_indices());
        panel_view.set_footer(&presentation::panel_footer_line(panel_state));
        self.status_label
            .set_label(&presentation::status_line(state));
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
        self.commander_view.focus_active_panel(active_panel);
    }

    pub(crate) fn clear_panel_focus_suppress_after(self: &Rc<Self>, duration: std::time::Duration) {
        let this = Rc::clone(self);
        glib::timeout_add_local_once(duration, move || {
            this.suppress_panel_focus.set(false);
        });
    }

    pub(crate) fn refresh_active_panel_revealing(self: &Rc<Self>, name: &str, status: String) {
        let navigation = self.navigation_controller();
        // Rename/create touch the directory; keep the watcher from rebuilding the
        // panel in the same moment as the intentional refresh.
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
                this.commander_view.refresh_lists_after_layout();
            });
    }

    fn set_navigation_busy(&self, panel: ActivePanel, busy: bool, message: &str) {
        self.navigation_runtime.navigation_busy.set(busy);
        self.busy_spinner.set_visible(busy);
        self.busy_spinner.set_spinning(busy);
        // Busy feedback stays in the status line; only the loading panel is gated.
        self.commander_view
            .panel(panel)
            .set_interaction_enabled(!busy);

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

fn effective_modifiers(
    key: gdk::Key,
    modifiers: gdk::ModifierType,
    pressed: bool,
) -> gdk::ModifierType {
    let mut modifiers = modifiers;
    let bit = match key {
        gdk::Key::Alt_L | gdk::Key::Alt_R | gdk::Key::Meta_L | gdk::Key::Meta_R => {
            gdk::ModifierType::ALT_MASK
        }
        gdk::Key::Control_L | gdk::Key::Control_R => gdk::ModifierType::CONTROL_MASK,
        gdk::Key::Shift_L | gdk::Key::Shift_R => gdk::ModifierType::SHIFT_MASK,
        _ => return modifiers,
    };
    if pressed {
        modifiers.insert(bit);
    } else {
        modifiers.remove(bit);
    }
    modifiers
}

fn command_bar_label_mode_from(modifiers: gdk::ModifierType) -> CommandBarLabelMode {
    if modifiers.contains(gdk::ModifierType::CONTROL_MASK)
        && modifiers.contains(gdk::ModifierType::SHIFT_MASK)
    {
        CommandBarLabelMode::Hidden
    } else if modifiers.contains(gdk::ModifierType::CONTROL_MASK) {
        CommandBarLabelMode::Ctrl
    } else if modifiers.contains(gdk::ModifierType::SHIFT_MASK) {
        CommandBarLabelMode::Shift
    } else if modifiers.contains(gdk::ModifierType::ALT_MASK) {
        CommandBarLabelMode::Alt
    } else {
        CommandBarLabelMode::Normal
    }
}
