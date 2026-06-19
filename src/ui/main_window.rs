use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};

#[cfg(not(target_os = "windows"))]
use std::path::PathBuf;

use gtk::{glib, prelude::*};
use rust_i18n::t;

use crate::{
    application::{ActivePanel, Commander, LoadScheduler, ViewUpdate},
    archive::ArchiveService,
    config::AppConfig,
    fs::watcher::{start_file_watcher, WatchCommand, WatchEvent},
    platform::assets::asset_path,
    presentation,
    ui::{
        commander_view::CommanderView,
        dialogs,
        operations::ActiveOperationHandle,
        shortcuts,
        terminal_dock::TerminalDock,
        theme::ThemeController,
    },
};

#[path = "main_window_actions.rs"]
mod actions;
#[path = "main_window_hosts.rs"]
mod hosts;
#[path = "main_window_navigation.rs"]
mod navigation_controller;
#[path = "main_window_panel_wiring.rs"]
mod panel_wiring;
#[path = "main_window_context_menu.rs"]
mod context_menu;
#[path = "main_window_operations.rs"]
mod operations_controller;
#[path = "main_window_terminal.rs"]
mod terminal_wiring;
#[path = "main_window_window_chrome.rs"]
mod window_chrome;

const APP_WINDOW_TITLE: &str = "RCommander";

use hosts::{NavigationHost, OperationsHost, ViewHost};
use context_menu::ContextMenuController;
use navigation_controller::NavigationController;
use operations_controller::OperationsController;
use panel_wiring::PanelWiring;
use terminal_wiring::TerminalController;
use window_chrome::{install_custom_window_controls, WindowChromeController};

pub struct MainWindow {
    pub window: gtk::ApplicationWindow,
    commander_view: CommanderView,
    terminal_dock: TerminalDock,
    content_paned: gtk::Paned,
    busy_spinner: gtk::Spinner,
    status_label: gtk::Label,
    commander: Rc<RefCell<Commander>>,
    archive_service: Rc<RefCell<ArchiveService>>,
    active_operation: Rc<RefCell<Option<ActiveOperationHandle>>>,
    navigation_busy: Rc<Cell<bool>>,
    watcher_refresh_cooldown_until: Rc<Cell<Option<Instant>>>,
    load_scheduler: Rc<RefCell<LoadScheduler>>,
    watch_command_tx: std::sync::mpsc::Sender<WatchCommand>,
    app_config_cache: Rc<RefCell<AppConfig>>,
    theme_controller: Rc<ThemeController>,
    #[cfg(not(target_os = "windows"))]
    unix_context_menu: Rc<RefCell<Option<gtk::Popover>>>,
}

impl MainWindow {
    pub fn new(app: &gtk::Application, commander: Commander, app_config: AppConfig) -> Rc<Self> {
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

        let header = gtk::HeaderBar::new();
        let title = gtk::Label::new(Some(APP_WINDOW_TITLE));
        title.add_css_class("app-title");
        header.set_title_widget(Some(&title));
        #[cfg(target_os = "windows")]
        install_custom_window_controls(&window, &header);
        window.set_titlebar(Some(&header));

        let shell = gtk::Box::new(gtk::Orientation::Vertical, 8);
        shell.set_margin_top(8);
        shell.set_margin_bottom(8);
        shell.set_margin_start(8);
        shell.set_margin_end(8);

        let commander_view = CommanderView::new();
        let initial_dir = commander.state().active_panel().location.host_directory();
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

        let commander = Rc::new(RefCell::new(commander));
        let archive_service = Rc::new(RefCell::new(ArchiveService::with_default_backends()));
        let (watch_command_tx, watch_event_rx) = start_file_watcher();

        let this = Rc::new(Self {
            window,
            commander_view,
            terminal_dock,
            content_paned,
            busy_spinner,
            status_label,
            commander,
            archive_service,
            active_operation: Rc::new(RefCell::new(None)),
            navigation_busy: Rc::new(Cell::new(false)),
            watcher_refresh_cooldown_until: Rc::new(Cell::new(None)),
            load_scheduler: Rc::new(RefCell::new(LoadScheduler::default())),
            watch_command_tx,
            app_config_cache: Rc::new(RefCell::new(app_config.clone())),
            theme_controller,
            #[cfg(not(target_os = "windows"))]
            unix_context_menu: Rc::new(RefCell::new(None)),
        });

        this.apply_update(ViewUpdate::all());
        this.window_chrome().apply_theme();
        this.window_chrome()
            .refresh_localized_labels(&this.commander_view, &this.terminal_dock);
        this.sync_watched_paths();
        this.panel_wiring().connect_panels(&this.commander_view);
        this.connect_command_bar(&command_bar);
        this.terminal_controller().connect_terminal_dock();
        this.install_watcher_poll(watch_event_rx);
        this.window_chrome().install_window_state_persistence();
        this.window_chrome().install_window_geometry_tracking();
        this.window_chrome().install_system_theme_tracking();
        shortcuts::install(&this, app);

        // Initialize Windows tray icon (no-op on other platforms)
        #[cfg(target_os = "windows")]
        {
            let _ = crate::platform::tray::create_tray_icon();
        }

        // On Windows: set the native window icon at runtime (WM_SETICON) so the taskbar updates immediately
        #[cfg(target_os = "windows")]
        {
            // Defer to idle so the window is realized
            glib::idle_add_local_once(move || {
                if let Err(error) = crate::platform::apply_runtime_window_icon(APP_WINDOW_TITLE) {
                    eprintln!("Could not apply Windows runtime icon: {error}");
                }
            });
        }

        this.window.present();
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let window = this.window.clone();
            glib::idle_add_local_once(move || {
                if let Err(error) = crate::platform::x11_window_icon::apply_window_icon(&window) {
                    eprintln!("Could not apply X11 window icon: {error}");
                }
            });
        }
        this.window_chrome().restore_window_geometry(window_config);
        this.window_chrome().initialize_split_positions();
        this.queue_initial_panel_loads();

        this
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

    fn install_watcher_poll(
        self: &Rc<Self>,
        watch_event_rx: std::sync::mpsc::Receiver<WatchEvent>,
    ) {
        let navigation = self.navigation_controller();
        glib::timeout_add_local(Duration::from_millis(350), move || {
            let mut changed_paths = Vec::new();
            while let Ok(event) = watch_event_rx.try_recv() {
                changed_paths.extend(event.paths);
            }

            if !changed_paths.is_empty() {
                let affected_panels = navigation.affected_panels_for_paths(&changed_paths);
                navigation.mark_panels_dirty(&affected_panels);
            }

            navigation.refresh_dirty_panels_if_idle();

            glib::ControlFlow::Continue
        });
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

        self.terminal_dock
            .set_panel_dir(state.active_panel().location.host_directory());
        self.terminal_dock.refresh_toolbar();
    }

    fn sync_watched_paths(&self) {
        let paths = self.commander.borrow().state().visible_paths();
        let _ = self.watch_command_tx.send(WatchCommand::SetPaths(paths));
    }

    fn active_panel_path(&self) -> std::path::PathBuf {
        self.commander
            .borrow()
            .state()
            .active_panel()
            .location
            .host_directory()
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
        self.navigation_busy.set(busy);
        self.busy_spinner.set_visible(busy);
        self.busy_spinner.set_spinning(busy);
        self.commander_view.set_interaction_enabled(!busy);

        if busy {
            self.status_label.set_label(message);
        } else {
            self.status_label
                .set_label(&presentation::status_line(self.commander.borrow().state()));
        }
    }

    fn queue_initial_panel_loads(self: &Rc<Self>) {
        self.navigation_controller().queue_initial_panel_loads();
    }

    fn navigation_controller(self: &Rc<Self>) -> NavigationController {
        let host: Rc<dyn NavigationHost> = self.clone();
        NavigationController::new(
            host,
            self.window.clone(),
            Rc::clone(&self.commander),
            Rc::clone(&self.archive_service),
            Rc::clone(&self.active_operation),
            Rc::clone(&self.navigation_busy),
            Rc::clone(&self.watcher_refresh_cooldown_until),
            Rc::clone(&self.load_scheduler),
            self.watch_command_tx.clone(),
            Rc::clone(&self.app_config_cache),
        )
    }

    fn panel_wiring(self: &Rc<Self>) -> PanelWiring {
        let host: Rc<dyn ViewHost> = self.clone();
        let this = Rc::clone(self);
        let context_menu_handler = Rc::new(move |panel, clicked_index, x, y| {
            this.context_menu_controller()
                .handle_panel_context_menu(panel, clicked_index, x, y);
        });
        PanelWiring::new(
            host,
            Rc::clone(&self.commander),
            self.navigation_controller(),
            context_menu_handler,
        )
    }

    fn context_menu_controller(self: &Rc<Self>) -> ContextMenuController {
        let host: Rc<dyn ViewHost> = self.clone();
        #[cfg(not(target_os = "windows"))]
        let open_action = {
            let this = Rc::clone(self);
            Rc::new(move || this.handle_open_active()) as Rc<dyn Fn()>
        };
        #[cfg(not(target_os = "windows"))]
        let rename_action = {
            let this = Rc::clone(self);
            Rc::new(move || this.handle_rename()) as Rc<dyn Fn()>
        };
        #[cfg(not(target_os = "windows"))]
        let copy_action = {
            let this = Rc::clone(self);
            Rc::new(move || this.handle_copy()) as Rc<dyn Fn()>
        };
        #[cfg(not(target_os = "windows"))]
        let move_action = {
            let this = Rc::clone(self);
            Rc::new(move || this.handle_move()) as Rc<dyn Fn()>
        };
        #[cfg(not(target_os = "windows"))]
        let delete_action = {
            let this = Rc::clone(self);
            Rc::new(move || this.handle_delete()) as Rc<dyn Fn()>
        };
        #[cfg(not(target_os = "windows"))]
        let mkdir_action = {
            let this = Rc::clone(self);
            Rc::new(move || this.handle_make_directory()) as Rc<dyn Fn()>
        };
        #[cfg(not(target_os = "windows"))]
        let chmod_action = {
            let this = Rc::clone(self);
            Rc::new(move |paths| this.handle_unix_chmod(paths)) as Rc<dyn Fn(Vec<PathBuf>)>
        };
        #[cfg(not(target_os = "windows"))]
        let chown_action = {
            let this = Rc::clone(self);
            Rc::new(move |paths| this.handle_unix_chown(paths)) as Rc<dyn Fn(Vec<PathBuf>)>
        };

        ContextMenuController::new(
            host,
            self.window.clone(),
            #[cfg(not(target_os = "windows"))]
            self.commander_view.left.root.clone(),
            #[cfg(not(target_os = "windows"))]
            self.commander_view.right.root.clone(),
            Rc::clone(&self.commander),
            #[cfg(not(target_os = "windows"))]
            Rc::clone(&self.unix_context_menu),
            #[cfg(not(target_os = "windows"))]
            open_action,
            #[cfg(not(target_os = "windows"))]
            rename_action,
            #[cfg(not(target_os = "windows"))]
            copy_action,
            #[cfg(not(target_os = "windows"))]
            move_action,
            #[cfg(not(target_os = "windows"))]
            delete_action,
            #[cfg(not(target_os = "windows"))]
            mkdir_action,
            #[cfg(not(target_os = "windows"))]
            chmod_action,
            #[cfg(not(target_os = "windows"))]
            chown_action,
        )
    }

    fn operations_controller(self: &Rc<Self>) -> OperationsController {
        let host: Rc<dyn OperationsHost> = self.clone();
        OperationsController::new(
            host,
            self.window.clone(),
            Rc::clone(&self.commander),
            Rc::clone(&self.archive_service),
            Rc::clone(&self.active_operation),
            Rc::clone(&self.app_config_cache),
            self.navigation_controller(),
        )
    }

    fn terminal_controller(self: &Rc<Self>) -> TerminalController {
        let host: Rc<dyn hosts::TerminalHost> = self.clone();
        TerminalController::new(
            host,
            self.terminal_dock.clone(),
            self.content_paned.clone(),
        )
    }

    fn window_chrome(&self) -> WindowChromeController {
        WindowChromeController::new(
            self.window.clone(),
            self.commander_view.root.clone(),
            self.content_paned.clone(),
            Rc::clone(&self.commander),
            Rc::clone(&self.app_config_cache),
            Rc::clone(&self.theme_controller),
        )
    }

    fn start_selected_navigation(self: &Rc<Self>, panel: ActivePanel) {
        self.navigation_controller().start_selected_navigation(panel);
    }

    fn handle_operation(self: &Rc<Self>, kind: crate::domain::operation::FileOperationKind) {
        self.operations_controller().handle_operation(kind);
    }

    fn handle_terminal_action(
        self: &Rc<Self>,
        action: crate::ui::terminal_controller::TerminalAction,
    ) {
        self.terminal_controller().handle_terminal_action(action);
    }
}

impl ViewHost for MainWindow {
    fn apply_update(&self, update: ViewUpdate) {
        MainWindow::apply_update(self, update);
    }

    fn show_error(&self, title: &str, detail: &str) {
        dialogs::show_error(&self.window, title, detail);
    }

    fn set_status(&self, status: String) {
        self.set_status_message(status);
    }
}

impl NavigationHost for MainWindow {
    fn set_navigation_busy(&self, busy: bool, message: &str) {
        MainWindow::set_navigation_busy(self, busy, message);
    }

    fn focus_active_panel(&self) {
        MainWindow::focus_active_panel(self);
    }

    fn apply_panel_root(&self, panel: ActivePanel) {
        self.commander_view
            .apply_root(self.commander.borrow().state(), panel);
    }
}

impl OperationsHost for MainWindow {}

impl hosts::TerminalHost for MainWindow {
    fn focus_active_panel(&self) {
        MainWindow::focus_active_panel(self);
    }
}

fn build_command_bar() -> gtk::Box {
    let command_bar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    command_bar.add_css_class("command-bar");
    command_bar.set_homogeneous(true);

    for label in command_bar_labels() {
        let button = gtk::Button::with_label(&label);
        button.add_css_class("command-button");
        command_bar.append(&button);
    }

    command_bar
}

fn command_bar_labels() -> Vec<String> {
    vec![
        t!("command.settings").into_owned(),
        t!("command.rename").into_owned(),
        t!("command.view").into_owned(),
        t!("command.edit").into_owned(),
        t!("command.copy").into_owned(),
        t!("command.move").into_owned(),
        t!("command.mkdir").into_owned(),
        t!("command.delete").into_owned(),
        t!("command.terminal").into_owned(),
        t!("command.quit").into_owned(),
    ]
}
