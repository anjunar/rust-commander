use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

use gtk::{glib, prelude::*};
use rust_i18n::t;

#[cfg(target_os = "windows")]
use crate::platform::restore_window_placement;

use crate::{
    application::{ActivePanel, Commander, LoadScheduler, ViewUpdate},
    archive::ArchiveService,
    config::{self, AppConfig, WindowConfig, WindowPosition},
    domain::sorting::SortDirection,
    fs::watcher::{start_file_watcher, WatchCommand, WatchEvent},
    platform::{assets::asset_path, current_window_placement, ContextMenuRequest},
    presentation,
    ui::{
        commander_view::CommanderView,
        dialogs,
        navigation::{self, NavigationRequest},
        operations::ActiveOperationHandle,
        shortcuts,
        terminal_controller::TerminalAction,
        terminal_dock::TerminalDock,
    },
};

#[path = "main_window_actions.rs"]
mod actions;
#[path = "main_window_navigation.rs"]
mod navigation_controller;
#[path = "main_window_operations.rs"]
mod operations_controller;

const APP_WINDOW_TITLE: &str = "RCommander";

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
}

impl MainWindow {
    pub fn new(app: &gtk::Application, commander: Commander, app_config: AppConfig) -> Rc<Self> {
        install_css();
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
        });

        this.apply_update(ViewUpdate::all());
        this.refresh_localized_labels();
        this.sync_watched_paths();
        this.connect_panel_events();
        this.connect_command_bar(&command_bar);
        this.connect_terminal_dock();
        this.install_watcher_poll(watch_event_rx);
        this.install_window_state_persistence();
        this.install_window_geometry_tracking();
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
        this.restore_window_geometry(window_config);
        this.initialize_split_positions();

        this
    }

    fn connect_panel_events(self: &Rc<Self>) {
        for panel in [ActivePanel::Left, ActivePanel::Right] {
            let panel_view = self.commander_view.panel(panel);

            {
                let this = Rc::clone(self);
                panel_view.connect_selection_changed(move |indices| {
                    let update = {
                        let mut commander = this.commander.borrow_mut();
                        commander.select_indices(panel, indices)
                    };
                    this.apply_update(update);
                });
            }

            {
                let this = Rc::clone(self);
                panel_view.connect_activate(move |index| {
                    let this = Rc::clone(&this);
                    glib::idle_add_local_once(move || {
                        let update = {
                            let mut commander = this.commander.borrow_mut();
                            commander.select_single(panel, index)
                        };
                        this.apply_update(update);
                        this.start_selected_navigation(panel);
                    });
                });
            }

            {
                let this = Rc::clone(self);
                panel_view.connect_open_key(move || {
                    let this = Rc::clone(&this);
                    glib::idle_add_local_once(move || {
                        this.start_selected_navigation(panel);
                    });
                });
            }

            {
                let this = Rc::clone(self);
                panel_view.connect_root_changed(move |index| {
                    let this = Rc::clone(&this);
                    glib::idle_add_local_once(move || {
                        this.start_root_navigation(panel, index);
                    });
                });
            }

            {
                let this = Rc::clone(self);
                panel_view.connect_secondary_click(move |clicked_index| {
                    let this = Rc::clone(&this);
                    glib::idle_add_local_once(move || {
                        this.handle_panel_context_menu(panel, clicked_index);
                    });
                });
            }

            {
                let this = Rc::clone(self);
                panel_view.connect_sort_changed(move |column, sort_type| {
                    let this = Rc::clone(&this);
                    glib::idle_add_local_once(move || {
                        let direction = match sort_type {
                            gtk::SortType::Descending => SortDirection::Descending,
                            _ => SortDirection::Ascending,
                        };
                        let update = {
                            let mut commander = this.commander.borrow_mut();
                            commander.sort_panel(panel, column, direction)
                        };
                        this.apply_update(update);
                    });
                });
            }
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

    fn connect_terminal_dock(self: &Rc<Self>) {
        {
            let this = Rc::clone(self);
            self.terminal_dock.connect_focus_return(move || {
                this.focus_active_panel();
            });
        }

        {
            let this = Rc::clone(self);
            self.terminal_dock.connect_buttons(move |action| {
                this.handle_terminal_action(action);
            });
        }
    }

    fn install_watcher_poll(
        self: &Rc<Self>,
        watch_event_rx: std::sync::mpsc::Receiver<WatchEvent>,
    ) {
        let this = Rc::clone(self);
        glib::timeout_add_local(Duration::from_millis(350), move || {
            let mut changed_paths = Vec::new();
            while let Ok(event) = watch_event_rx.try_recv() {
                changed_paths.extend(event.paths);
            }

            if !changed_paths.is_empty() {
                let affected_panels = this.affected_panels_for_paths(&changed_paths);
                this.mark_panels_dirty(&affected_panels);
            }

            this.refresh_dirty_panels_if_idle();

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

    fn prepare_navigation_request(&self, request: NavigationRequest) -> NavigationRequest {
        self.load_scheduler.borrow_mut().prepare_request(request)
    }

    fn commit_loaded_generation(&self, panel: ActivePanel, generation: u64) -> bool {
        self.load_scheduler
            .borrow_mut()
            .commit_loaded(panel, generation)
    }

    fn finish_in_flight_load(&self, panel: ActivePanel, generation: u64) {
        self.load_scheduler
            .borrow_mut()
            .finish_in_flight(panel, generation);
    }

    fn mark_panels_dirty(&self, panels: &[ActivePanel]) {
        self.load_scheduler
            .borrow_mut()
            .queue_refresh(panels, t!("status.view_refreshed").into_owned());
    }

    fn refresh_dirty_panels_if_idle(self: &Rc<Self>) {
        if self.active_operation.borrow().is_some()
            || self.navigation_busy.get()
            || self.is_watcher_refresh_suppressed()
        {
            return;
        }

        let Some((panel, status)) = self
            .load_scheduler
            .borrow_mut()
            .take_next_refresh(&t!("status.view_refreshed").into_owned())
        else {
            return;
        };
        let request = {
            let commander = self.commander.borrow();
            navigation::refresh_request(&commander, panel, status)
        };
        self.start_directory_load(request);
    }

    fn queue_async_refresh_panels(
        self: &Rc<Self>,
        panels: &[ActivePanel],
        status: impl Into<String>,
    ) {
        if panels.is_empty() {
            return;
        }

        self.load_scheduler
            .borrow_mut()
            .queue_refresh(panels, status.into());
        self.refresh_dirty_panels_if_idle();
    }

    fn queue_async_refresh_for_paths(
        self: &Rc<Self>,
        changed_paths: &[PathBuf],
        status: impl Into<String>,
    ) {
        let panels = self.affected_panels_for_paths(changed_paths);
        if panels.is_empty() {
            return;
        }
        self.queue_async_refresh_panels(&panels, status);
    }

    fn trigger_manual_refresh_cooldown(&self) {
        self.watcher_refresh_cooldown_until
            .set(Some(Instant::now() + Duration::from_millis(900)));
        self.sync_watched_paths();
    }

    fn is_watcher_refresh_suppressed(&self) -> bool {
        match self.watcher_refresh_cooldown_until.get() {
            Some(until) if Instant::now() < until => true,
            Some(_) => {
                self.watcher_refresh_cooldown_until.set(None);
                false
            }
            None => false,
        }
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

    fn handle_terminal_action(&self, action: TerminalAction) {
        match action {
            TerminalAction::None => {
                self.terminal_dock.sync_visibility();
                if self.terminal_dock.state().borrow().visible
                    && self.content_paned.position() < 320
                {
                    self.content_paned.set_position(600);
                }
            }
            TerminalAction::FocusPanels => self.focus_active_panel(),
            TerminalAction::ShowError(error) => {
                dialogs::show_error(&self.window, &t!("error.could_not_start_terminal"), &error);
            }
        }
    }

    fn handle_panel_context_menu(
        self: &Rc<Self>,
        panel: ActivePanel,
        clicked_index: Option<usize>,
    ) {
        let (request, update) = {
            let mut commander = self.commander.borrow_mut();
            let mut update = commander.set_active_panel(panel);
            if let Some(index) = clicked_index {
                let keep_multi_selection = commander
                    .state()
                    .panel(panel)
                    .selection_indices()
                    .contains(&index);
                if !keep_multi_selection {
                    update = commander.select_single(panel, index);
                }
            }
            let panel_state = commander.state().panel(panel);
            let Some(directory) = panel_state.location.filesystem_path().map(PathBuf::from) else {
                dialogs::show_error(
                    &self.window,
                    &t!("error.command_failed"),
                    "The native context menu is currently only available in filesystem views.",
                );
                return;
            };

            let selected_paths = panel_state
                .selected_items()
                .into_iter()
                .filter(|item| item.archive_path.is_none())
                .map(|item| item.path)
                .collect::<Vec<_>>();

            (
                ContextMenuRequest {
                    directory,
                    selected_paths,
                },
                update,
            )
        };

        self.apply_update(update);

        if let Err(error) = crate::platform::show_context_menu(&request) {
            self.show_command_failed(error);
            return;
        }

        self.queue_async_refresh_panels(&[panel], t!("status.view_refreshed").into_owned());
    }

    fn affected_panels_for_paths(&self, changed_paths: &[PathBuf]) -> Vec<ActivePanel> {
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

    fn install_window_state_persistence(self: &Rc<Self>) {
        let commander = Rc::clone(&self.commander);
        let window = self.window.clone();
        let app_config_cache = Rc::clone(&self.app_config_cache);
        self.window.connect_close_request(move |_| {
            {
                let commander = commander.borrow();
                let mut app_config = app_config_cache.borrow_mut();
                app_config.window.maximized = window.is_maximized();
                if !app_config.window.maximized {
                    app_config.window.width = window.width().max(1);
                    app_config.window.height = window.height().max(1);
                }
                app_config.panes.left_directory =
                    Some(commander.panel_directory(ActivePanel::Left));
                app_config.panes.right_directory =
                    Some(commander.panel_directory(ActivePanel::Right));
            }

            if let Err(error) = config::save(&app_config_cache.borrow().clone()) {
                eprintln!("Could not save config: {error}");
            }
            glib::Propagation::Proceed
        });
    }

    fn install_window_geometry_tracking(self: &Rc<Self>) {
        let window = self.window.clone();
        let app_config_cache = Rc::clone(&self.app_config_cache);
        glib::timeout_add_local(Duration::from_millis(250), move || {
            let mut app_config = app_config_cache.borrow_mut();
            let config = &mut app_config.window;
            config.maximized = window.is_maximized();
            if let Some(placement) = current_window_placement(APP_WINDOW_TITLE) {
                config.width = placement.width.max(1);
                config.height = placement.height.max(1);
                config.position = Some(WindowPosition {
                    x: placement.x,
                    y: placement.y,
                });
                config.maximized = placement.maximized;
            } else if !config.maximized {
                config.width = window.width().max(1);
                config.height = window.height().max(1);
            }
            glib::ControlFlow::Continue
        });
    }

    fn restore_window_geometry(&self, window_config: WindowConfig) {
        #[cfg(not(target_os = "windows"))]
        {
            self.window
                .set_default_size(window_config.width.max(1), window_config.height.max(1));
            if window_config.maximized {
                let window = self.window.clone();
                glib::idle_add_local_once(move || {
                    window.maximize();
                });
            }
        }

        #[cfg(target_os = "windows")]
        {
            let position = window_config
                .position
                .unwrap_or(WindowPosition { x: 0, y: 0 });
            glib::idle_add_local_once({
                let position = position.clone();
                let width = window_config.width;
                let height = window_config.height;
                let maximized = window_config.maximized;
                move || {
                    restore_window_placement(
                        APP_WINDOW_TITLE,
                        position.x,
                        position.y,
                        width,
                        height,
                        maximized,
                    );
                }
            });
            let width = window_config.width;
            let height = window_config.height;
            let maximized = window_config.maximized;
            glib::timeout_add_local_once(Duration::from_millis(150), move || {
                restore_window_placement(
                    APP_WINDOW_TITLE,
                    position.x,
                    position.y,
                    width,
                    height,
                    maximized,
                );
            });
        }
    }

    fn initialize_split_positions(&self) {
        let horizontal = self.commander_view.root.clone();
        let vertical = self.content_paned.clone();
        glib::timeout_add_local_once(Duration::from_millis(30), move || {
            let horizontal_width = horizontal.width();
            if horizontal_width > 0 {
                horizontal.set_position(horizontal_width / 2);
            }

            let vertical_height = vertical.height();
            if vertical_height > 0 {
                vertical.set_position(vertical_height / 2);
            }
        });
    }

    fn refresh_localized_labels(&self) {
        self.commander_view.refresh_labels();
        self.terminal_dock.refresh_toolbar();
        if let Some(titlebar) = self.window.titlebar() {
            if let Ok(header) = titlebar.downcast::<gtk::HeaderBar>() {
                if let Some(title_widget) = header.title_widget() {
                    if let Ok(label) = title_widget.downcast::<gtk::Label>() {
                        label.set_label(APP_WINDOW_TITLE);
                    }
                }
            }
        }

        let labels = command_bar_labels();
        let mut index = 0usize;
        let mut child = self
            .window
            .child()
            .and_then(|child| child.downcast::<gtk::Box>().ok())
            .and_then(|shell| shell.last_child());
        while let Some(widget) = child {
            let previous = widget.prev_sibling();
            if let Ok(button_row) = widget.clone().downcast::<gtk::Box>() {
                let mut button = button_row.first_child();
                while let Some(widget) = button {
                    button = widget.next_sibling();
                    if let Ok(button) = widget.downcast::<gtk::Button>() {
                        if let Some(label) = labels.get(index) {
                            button.set_label(label);
                        }
                        index += 1;
                    }
                }
                break;
            }
            child = previous;
        }
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

fn install_css() {
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_application_prefer_dark_theme(true);
    }

    let provider = gtk::CssProvider::new();
    provider.load_from_string(
        "
        /* Darker IntelliJ Darcula variant */
        window,
        dialog,
        popover,
        menupopover,
        .background {
            background: #181818; /* darker main window */
            color: #a9b7c6; /* main text */
        }

        headerbar {
            background: #262626; /* darker header */
            color: #a9b7c6;
            border-bottom: 1px solid #2c2c2c;
            box-shadow: none;
        }

        headerbar:backdrop {
            background: #262626;
            color: #a9b7c6;
        }

        /* Make minimize/maximize/close titlebuttons match the window background */
        headerbar .titlebutton,
        window .titlebutton,
        .titlebutton {
            background: #181818;
            background-color: #181818; /* match main window background */
            border: none;
            box-shadow: none;
            color: inherit;
        }

        /* Ensure hover/active use the same background (no contrasting highlight) */
        headerbar .titlebutton:hover,
        window .titlebutton:hover,
        .titlebutton:hover,
        headerbar .titlebutton:active,
        .titlebutton:active {
            background: #181818;
            background-color: #181818;
            box-shadow: none;
        }

        .app-title {
            font-weight: 700;
            letter-spacing: 0.06em;
        }

        .commander-view {
            background: linear-gradient(135deg, #181818, #121212);
        }

        .file-panel {
            padding: 10px;
            border: 1px solid #2b2b2b;
            border-radius: 10px;
            background: #1f1f20;
        }

        .active-panel {
            border-color: #2a7fd1; /* keep selection accent */
            box-shadow: 0 0 0 1px rgba(42,127,209,0.24);
        }

        .path-row {
            padding: 2px 0 6px 0;
        }

        dropdown,
        dropdown button,
        entry,
        dialog entry,
        button,
        menubutton button {
            background: #141414;
            color: #a9b7c6;
            border-color: #2b2b2b;
        }

        dropdown button:hover,
        button:hover,
        menubutton button:hover {
            background: #1f1f1f;
        }

        .root-selector,
        .root-selector button {
            border-radius: 8px;
        }

        dropdown button:focus,
        button:focus,
        entry:focus {
            box-shadow: 0 0 0 1px rgba(42,127,209,0.24);
            border-color: #2a7fd1;
        }

        .path-label {
            font-family: 'Cascadia Code', 'Consolas', monospace;
            font-size: 0.95em;
            color: #bdbdbd;
            padding: 7px 10px;
            border-radius: 8px;
            background: #141414;
            border: 1px solid #232323;
        }

        scrolledwindow,
        scrolledwindow > viewport,
        .file-table,
        columnview,
        listview,
        listview.view,
        widget.view {
            background: #141414;
            color: #a9b7c6;
        }

        columnview header,
        columnview header button,
        columnview columnheader,
        columnview columnheader button {
            background: #1d1d1d;
            color: #c8d6e5;
            border-color: #252525;
            box-shadow: none;
            font-weight: 700;
        }

        columnview row {
            color: #a9b7c6;
        }

        columnview row:nth-child(even) {
            background: rgba(255,255,255,0.01);
        }

        columnview row:hover {
            background: rgba(255,255,255,0.02);
        }

        columnview row:selected,
        listview row:selected,
        treeexpander row:selected {
            background: #214283; /* keep selection */
            color: #ffffff;
        }

        .parent-link {
            color: #6296c9;
            font-style: italic;
        }

        separator {
            background: #232323;
        }

        .panel-scroller {
            border: 1px solid #232323;
            border-radius: 8px;
            background: #141414;
        }

        .panel-scroller > viewport {
            border-radius: 8px;
        }

        .editor-view,
        .editor-view text,
        .editor-view border,
        .editor-view gutter {
            background: #141414;
            color: #a9b7c6;
            font-family: 'Fira Code', 'Cascadia Code', 'Source Code Pro', monospace;
            font-size: 13px;
        }

        .editor-status {
            color: #9fb1c3;
            font-family: 'Cascadia Code', 'Consolas', monospace;
        }

        .terminal-dock {
            border: 1px solid #1f1f1f;
            border-radius: 10px;
            background: #0f0f0f;
        }

        .terminal-toolbar {
            border-bottom: 1px solid #1a1a1a;
            background: rgba(20,20,20,0.95);
        }

        .terminal-title {
            font-weight: 700;
            letter-spacing: 0.04em;
        }

        .terminal-cwd {
            font-family: 'Cascadia Code', 'Consolas', monospace;
            color: #9fb1c3;
        }

        .terminal-button {
            font-family: 'Cascadia Code', 'Consolas', monospace;
        }

        .terminal-placeholder {
            background: #141414;
        }

        .terminal-placeholder-title {
            font-weight: 700;
        }

        .terminal-placeholder-copy {
            color: #9eaec0;
        }

        scrollbar slider {
            background: #2b2b2b;
            border-radius: 999px;
            min-width: 10px;
            min-height: 10px;
        }

        popover contents,
        dialog > box,
        messagedialog box,
        .dialog-action-area {
            background: #141414;
            color: #a9b7c6;
        }

        .status-line {
            padding: 6px 10px;
            border-radius: 8px;
            background: #131313;
            color: #d0d6db;
            font-family: 'Cascadia Code', 'Consolas', monospace;
            border: 1px solid #1f1f1f;
        }

        .command-bar {
            padding-top: 2px;
        }

        .command-button {
            font-family: 'Cascadia Code', 'Consolas', monospace;
            font-weight: 700;
            background: #1d1d1d;
            color: #a9b7c6;
            border: 1px solid #232323;
            border-radius: 8px;
            padding: 10px 14px;
        }

        .dialog-title {
            font-weight: 700;
            font-size: 1.1em;
        }
        ",
    );

    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
