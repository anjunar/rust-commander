use std::{cell::RefCell, rc::Rc, time::Duration};

use gtk::{glib, prelude::*};

use crate::{
    application::{ActivePanel, Commander, ViewUpdate},
    domain::{
        operation::{FileOperationKind, FileOperationRequest, OperationEvent},
        sorting::SortDirection,
    },
    fs::{
        operations::{OperationHandle, start_operation},
        watcher::{WatchCommand, start_file_watcher},
    },
    ui::{
        commander_view::CommanderView, dialogs, editor_dialog, shortcuts,
        terminal_controller::TerminalAction, terminal_dock::TerminalDock,
    },
};

pub struct MainWindow {
    pub window: gtk::ApplicationWindow,
    commander_view: CommanderView,
    terminal_dock: TerminalDock,
    content_paned: gtk::Paned,
    status_label: gtk::Label,
    commander: Rc<RefCell<Commander>>,
    active_operation: Rc<RefCell<Option<OperationHandle>>>,
    watch_command_tx: std::sync::mpsc::Sender<WatchCommand>,
}

impl MainWindow {
    pub fn new(app: &gtk::Application, commander: Commander) -> Rc<Self> {
        install_css();

        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("RCommander")
            .default_width(1180)
            .default_height(760)
            .build();

        let header = gtk::HeaderBar::new();
        let title = gtk::Label::new(Some("RCommander"));
        title.add_css_class("app-title");
        header.set_title_widget(Some(&title));
        window.set_titlebar(Some(&header));

        let shell = gtk::Box::new(gtk::Orientation::Vertical, 8);
        shell.set_margin_top(8);
        shell.set_margin_bottom(8);
        shell.set_margin_start(8);
        shell.set_margin_end(8);

        let commander_view = CommanderView::new();
        let initial_dir = commander.state().active_panel().path.clone();
        let terminal_dock = TerminalDock::new(initial_dir);

        let content_paned = gtk::Paned::new(gtk::Orientation::Vertical);
        content_paned.set_hexpand(true);
        content_paned.set_vexpand(true);
        content_paned.set_resize_start_child(true);
        content_paned.set_resize_end_child(false);
        content_paned.set_shrink_start_child(false);
        content_paned.set_shrink_end_child(false);
        content_paned.set_position(600);
        content_paned.set_start_child(Some(&commander_view.root));
        content_paned.set_end_child(Some(&terminal_dock.root));
        shell.append(&content_paned);

        let status_label = gtk::Label::new(None);
        status_label.set_xalign(0.0);
        status_label.add_css_class("status-line");
        shell.append(&status_label);

        let command_bar = build_command_bar();
        shell.append(&command_bar);

        window.set_child(Some(&shell));

        let commander = Rc::new(RefCell::new(commander));
        let (watch_command_tx, watch_event_rx) = start_file_watcher();

        let this = Rc::new(Self {
            window,
            commander_view,
            terminal_dock,
            content_paned,
            status_label,
            commander,
            active_operation: Rc::new(RefCell::new(None)),
            watch_command_tx,
        });

        this.apply_update(ViewUpdate::all());
        this.sync_watched_paths();
        this.connect_panel_events();
        this.connect_command_bar(&command_bar);
        this.connect_terminal_dock();
        this.install_watcher_poll(watch_event_rx);
        shortcuts::install(&this, app);
        this.window.present();

        this
    }

    pub fn handle_switch_panel(self: &Rc<Self>) {
        let update = {
            let mut commander = self.commander.borrow_mut();
            commander.switch_panel()
        };
        self.apply_update(update);
    }

    pub fn handle_open_active(self: &Rc<Self>) {
        let active_panel = self.commander.borrow().state().active_panel;
        self.run_command(|commander| commander.activate_selected(active_panel));
    }

    pub fn handle_view(self: &Rc<Self>) {
        let selected = self
            .commander
            .borrow()
            .state()
            .active_panel()
            .selected_item();

        let Some(selected) = selected else {
            dialogs::show_error(
                &self.window,
                "View is not available",
                "No entry is selected.",
            );
            return;
        };

        if selected.is_parent_link {
            dialogs::show_error(
                &self.window,
                "View is not available",
                "The parent directory entry cannot be viewed.",
            );
            return;
        }

        if selected.is_dir {
            dialogs::show_error(
                &self.window,
                "View is not available",
                "Directories cannot be viewed in the file viewer.",
            );
            return;
        }

        if let Err(error) = editor_dialog::view_file(&self.window, selected.path.clone()) {
            dialogs::show_error(&self.window, "Could not open viewer", &error.to_string());
        }
    }

    pub fn handle_rename(self: &Rc<Self>) {
        let selected = self
            .commander
            .borrow()
            .state()
            .active_panel()
            .selected_item();

        let Some(selected) = selected else {
            dialogs::show_error(
                &self.window,
                "Rename is not available",
                "No entry is selected.",
            );
            return;
        };

        if selected.is_parent_link {
            dialogs::show_error(
                &self.window,
                "Rename is not available",
                "The parent directory entry cannot be renamed.",
            );
            return;
        }

        let this = Rc::clone(self);
        dialogs::prompt_rename(&self.window, selected.display_name, move |new_name| {
            this.run_command(|commander| commander.rename_active(&new_name));
        });
    }

    pub fn handle_open_console(self: &Rc<Self>) {
        let path = self.commander.borrow().state().active_panel().path.clone();

        if let Err(error) = crate::platform::open_console(&path) {
            dialogs::show_error(&self.window, "Could not open console", &error.to_string());
            return;
        }

        let update = {
            let mut commander = self.commander.borrow_mut();
            commander.set_status(format!("Console opened at {}", path.display()))
        };
        self.apply_update(update);
    }

    pub fn handle_help(self: &Rc<Self>) {
        let update = {
            let mut commander = self.commander.borrow_mut();
            commander.set_status(
                "F2 Rename, F3 View, F4 Edit, F5 Copy, F6 Move, F7 MkDir, F8 Delete, F9 Terminal, Tab Switch, Enter Open. F1 Help is a placeholder for an upcoming action.",
            )
        };
        self.apply_update(update);
    }

    pub fn handle_copy(self: &Rc<Self>) {
        self.handle_operation(FileOperationKind::Copy);
    }

    pub fn handle_toggle_terminal(self: &Rc<Self>) {
        if !self.terminal_dock.is_supported() {
            self.handle_open_console();
            return;
        }
        self.terminal_dock.set_panel_dir(self.active_panel_path());
        self.handle_terminal_action(self.terminal_dock.toggle());
    }

    pub fn handle_focus_terminal(self: &Rc<Self>) {
        if !self.terminal_dock.is_supported() {
            self.handle_open_console();
            return;
        }
        self.terminal_dock.set_panel_dir(self.active_panel_path());
        self.handle_terminal_action(self.terminal_dock.focus_terminal());
    }

    pub fn handle_restart_terminal(self: &Rc<Self>) {
        if !self.terminal_dock.is_supported() {
            self.handle_open_console();
            return;
        }
        self.terminal_dock.set_panel_dir(self.active_panel_path());
        self.handle_terminal_action(self.terminal_dock.restart_in_panel_dir());
    }

    pub fn handle_edit(self: &Rc<Self>) {
        let selected = self
            .commander
            .borrow()
            .state()
            .active_panel()
            .selected_item();

        let Some(selected) = selected else {
            dialogs::show_error(
                &self.window,
                "Edit is not available",
                "No entry is selected.",
            );
            return;
        };

        if selected.is_parent_link {
            dialogs::show_error(
                &self.window,
                "Edit is not available",
                "The parent directory entry cannot be edited.",
            );
            return;
        }

        if selected.is_dir {
            dialogs::show_error(
                &self.window,
                "Edit is not available",
                "Directories cannot be opened in the text editor.",
            );
            return;
        }

        let this = Rc::clone(self);
        if let Err(error) =
            editor_dialog::edit_file(&self.window, selected.path.clone(), move |path| {
                let update = {
                    let mut commander = this.commander.borrow_mut();
                    commander.refresh_with_status(format!("Saved: {}", path.display()))
                };
                this.apply_update(update);
                this.sync_watched_paths();
            })
        {
            dialogs::show_error(&self.window, "Could not open editor", &error.to_string());
        }
    }

    pub fn handle_move(self: &Rc<Self>) {
        self.handle_operation(FileOperationKind::Move);
    }

    pub fn handle_delete(self: &Rc<Self>) {
        self.handle_operation(FileOperationKind::Delete);
    }

    pub fn handle_make_directory(self: &Rc<Self>) {
        let this = Rc::clone(self);
        dialogs::prompt_new_directory(&self.window, move |name| {
            this.run_command(|commander| commander.create_directory_in_active(&name));
            this.sync_watched_paths();
        });
    }

    pub fn handle_quit(self: &Rc<Self>) {
        self.window.close();
    }

    fn handle_operation(self: &Rc<Self>, kind: FileOperationKind) {
        if self.active_operation.borrow().is_some() {
            dialogs::show_error(
                &self.window,
                "File operation already running",
                "Cancel or finish the current operation first.",
            );
            return;
        }

        let request = match self.commander.borrow().operation_request(kind) {
            Ok(request) => request,
            Err(error) => {
                dialogs::show_error(
                    &self.window,
                    "Operation is not available",
                    &error.to_string(),
                );
                return;
            }
        };

        let this = Rc::clone(self);
        dialogs::confirm_operation(&self.window, request, move |request| {
            this.start_file_operation(request);
        });
    }

    fn start_file_operation(self: &Rc<Self>, request: FileOperationRequest) {
        let (handle, receiver) = start_operation(request.clone());
        self.active_operation.borrow_mut().replace(handle.clone());

        let active_operation = Rc::clone(&self.active_operation);
        let progress_dialog = dialogs::ProgressDialog::new(
            &self.window,
            &format!("{} operation", request.kind.label()),
            move || {
                if let Some(handle) = active_operation.borrow().as_ref() {
                    handle.cancel();
                }
            },
        );

        let this = Rc::clone(self);
        glib::timeout_add_local(Duration::from_millis(80), move || {
            let mut keep_running = true;

            while let Ok(event) = receiver.try_recv() {
                match event {
                    OperationEvent::Progress(snapshot) => {
                        progress_dialog.update_progress(&snapshot);
                        let update = {
                            let mut commander = this.commander.borrow_mut();
                            commander.set_status(format!(
                                "{}: {}",
                                snapshot.kind.label(),
                                snapshot.current_item
                            ))
                        };
                        this.apply_update(update);
                    }
                    OperationEvent::Conflict(conflict) => {
                        progress_dialog.set_waiting_for_conflict();
                        let handle = this.active_operation.borrow().clone();
                        dialogs::show_conflict(&this.window, conflict, move |resolution| {
                            if let Some(handle) = handle.as_ref() {
                                handle.resolve_conflict(resolution);
                            }
                        });
                    }
                    OperationEvent::Finished(summary) => {
                        progress_dialog.close();
                        this.active_operation.borrow_mut().take();
                        let status = format!(
                            "{} completed: {} items, {} in {:.1}s",
                            summary.kind.label(),
                            summary.total_entries,
                            crate::fs::reader::format_bytes(summary.total_bytes),
                            summary.elapsed.as_secs_f64()
                        );
                        let update = {
                            let mut commander = this.commander.borrow_mut();
                            commander.refresh_after_operation(status)
                        };
                        this.apply_update(update);
                        this.sync_watched_paths();
                        keep_running = false;
                    }
                    OperationEvent::Cancelled(summary) => {
                        progress_dialog.close();
                        this.active_operation.borrow_mut().take();
                        let status = format!(
                            "{} cancelled after {} items and {}.",
                            summary.kind.label(),
                            summary.total_entries,
                            crate::fs::reader::format_bytes(summary.total_bytes)
                        );
                        let update = {
                            let mut commander = this.commander.borrow_mut();
                            commander.refresh_after_operation(status)
                        };
                        this.apply_update(update);
                        this.sync_watched_paths();
                        keep_running = false;
                    }
                    OperationEvent::Failed(error) => {
                        progress_dialog.close();
                        this.active_operation.borrow_mut().take();
                        let update = {
                            let mut commander = this.commander.borrow_mut();
                            commander.set_status(format!("File operation failed: {error}"))
                        };
                        this.apply_update(update);
                        dialogs::show_error(&this.window, "File operation failed", &error);
                        keep_running = false;
                    }
                }
            }

            if keep_running {
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
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
                        this.run_command(|commander| commander.activate_index(panel, index));
                        this.sync_watched_paths();
                    });
                });
            }

            {
                let this = Rc::clone(self);
                panel_view.connect_root_changed(move |index| {
                    let this = Rc::clone(&this);
                    glib::idle_add_local_once(move || {
                        this.run_command(|commander| commander.change_root(panel, index));
                        this.sync_watched_paths();
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

    fn install_watcher_poll(self: &Rc<Self>, watch_event_rx: std::sync::mpsc::Receiver<()>) {
        let this = Rc::clone(self);
        glib::timeout_add_local(Duration::from_millis(350), move || {
            let mut has_changes = false;
            while watch_event_rx.try_recv().is_ok() {
                has_changes = true;
            }

            if has_changes && this.active_operation.borrow().is_none() {
                this.run_command(|commander| commander.refresh_visible_panels());
                this.sync_watched_paths();
            }

            glib::ControlFlow::Continue
        });
    }

    fn run_command<F>(self: &Rc<Self>, command: F)
    where
        F: FnOnce(&mut Commander) -> anyhow::Result<ViewUpdate>,
    {
        let result = {
            let mut commander = self.commander.borrow_mut();
            command(&mut commander)
        };

        match result {
            Ok(update) => self.apply_update(update),
            Err(error) => {
                let update = {
                    let mut commander = self.commander.borrow_mut();
                    commander.set_status(format!("Command failed: {error}"))
                };
                self.apply_update(update);
                dialogs::show_error(&self.window, "Command failed", &error.to_string());
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
            self.status_label.set_label(&state.status_line());
        }

        self.terminal_dock
            .set_panel_dir(state.active_panel().path.clone());
        self.terminal_dock.refresh_toolbar();
    }

    fn sync_watched_paths(&self) {
        let paths = self.commander.borrow().state().visible_paths();
        let _ = self.watch_command_tx.send(WatchCommand::SetPaths(paths));
    }

    fn active_panel_path(&self) -> std::path::PathBuf {
        self.commander.borrow().state().active_panel().path.clone()
    }

    fn focus_active_panel(&self) {
        let active_panel = self.commander.borrow().state().active_panel;
        self.commander_view.focus_active_panel(active_panel);
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
                dialogs::show_error(&self.window, "Could not start terminal", &error);
            }
        }
    }
}

fn build_command_bar() -> gtk::Box {
    let command_bar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    command_bar.add_css_class("command-bar");
    command_bar.set_homogeneous(true);

    for label in [
        "F1 Help",
        "F2 Rename",
        "F3 View",
        "F4 Edit",
        "F5 Copy",
        "F6 Move",
        "F7 MkDir",
        "F8 Delete",
        "F9 Terminal",
        "F10 Quit",
    ] {
        let button = gtk::Button::with_label(label);
        button.add_css_class("command-button");
        command_bar.append(&button);
    }

    command_bar
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
