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
    ui::{commander_view::CommanderView, dialogs, shortcuts},
};

pub struct MainWindow {
    pub window: gtk::ApplicationWindow,
    commander_view: CommanderView,
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
        shell.append(&commander_view.root);

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
            status_label,
            commander,
            active_operation: Rc::new(RefCell::new(None)),
            watch_command_tx,
        });

        this.apply_update(ViewUpdate::all());
        this.sync_watched_paths();
        this.connect_panel_events();
        this.connect_command_bar(&command_bar);
        this.install_watcher_poll(watch_event_rx);
        shortcuts::install(&this, app);
        this.window.present();

        this
    }

    pub fn handle_switch_panel(self: &Rc<Self>) {
        let update = self.commander.borrow_mut().switch_panel();
        self.apply_update(update);
    }

    pub fn handle_open_active(self: &Rc<Self>) {
        let active_panel = self.commander.borrow().state().active_panel;
        self.run_command(|commander| commander.activate_selected(active_panel));
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

    pub fn handle_copy(self: &Rc<Self>) {
        self.handle_operation(FileOperationKind::Copy);
    }

    pub fn handle_move(self: &Rc<Self>) {
        self.handle_operation(FileOperationKind::Move);
    }

    pub fn handle_delete(self: &Rc<Self>) {
        self.handle_operation(FileOperationKind::Delete);
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
                        let update = this.commander.borrow_mut().set_status(format!(
                            "{}: {}",
                            snapshot.kind.label(),
                            snapshot.current_item
                        ));
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
                        let update = this.commander.borrow_mut().refresh_after_operation(status);
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
                        let update = this.commander.borrow_mut().refresh_after_operation(status);
                        this.apply_update(update);
                        this.sync_watched_paths();
                        keep_running = false;
                    }
                    OperationEvent::Failed(error) => {
                        progress_dialog.close();
                        this.active_operation.borrow_mut().take();
                        let update = this
                            .commander
                            .borrow_mut()
                            .set_status(format!("File operation failed: {error}"));
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
                    let update = this.commander.borrow_mut().select_indices(panel, indices);
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
                        let update = this
                            .commander
                            .borrow_mut()
                            .sort_panel(panel, column, direction);
                        this.apply_update(update);
                    });
                });
            }
        }
    }

    fn connect_command_bar(self: &Rc<Self>, command_bar: &gtk::Box) {
        let callbacks: [fn(&Rc<Self>); 6] = [
            Self::handle_rename,
            Self::handle_copy,
            Self::handle_move,
            Self::handle_delete,
            Self::handle_switch_panel,
            Self::handle_open_active,
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
        match command(&mut self.commander.borrow_mut()) {
            Ok(update) => self.apply_update(update),
            Err(error) => {
                let update = self
                    .commander
                    .borrow_mut()
                    .set_status(format!("Command failed: {error}"));
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
            self.status_label.set_label(&state.selection_summary());
        }
    }

    fn sync_watched_paths(&self) {
        let paths = self.commander.borrow().state().visible_paths();
        let _ = self.watch_command_tx.send(WatchCommand::SetPaths(paths));
    }
}

fn build_command_bar() -> gtk::Box {
    let command_bar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    command_bar.add_css_class("command-bar");
    command_bar.set_homogeneous(true);

    for label in [
        "F2 Rename",
        "F5 Copy",
        "F6 Move",
        "F8 Delete",
        "Tab Switch",
        "Enter Open",
    ] {
        let button = gtk::Button::with_label(label);
        button.add_css_class("command-button");
        command_bar.append(&button);
    }

    command_bar
}

fn install_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(
        "
        window {
            background: #111820;
            color: #e6edf3;
        }

        .app-title {
            font-weight: 700;
            letter-spacing: 0.06em;
        }

        .commander-view {
            background: linear-gradient(135deg, #111820, #1b2733);
        }

        .file-panel {
            padding: 8px;
            border: 1px solid #314050;
            border-radius: 10px;
            background: #16212b;
        }

        .active-panel {
            border-color: #e0a93b;
            box-shadow: 0 0 0 1px rgba(224, 169, 59, 0.38);
        }

        .path-row {
            padding-bottom: 4px;
        }

        .path-label {
            font-family: 'Cascadia Code', 'Consolas', monospace;
            font-size: 0.95em;
            color: #c8d6e5;
        }

        .file-table {
            background: #101820;
        }

        columnview row:selected {
            background: #315d7f;
        }

        .parent-link {
            color: #8fc7ff;
            font-style: italic;
        }

        .status-line {
            padding: 6px 10px;
            border-radius: 8px;
            background: #0d131a;
            color: #d8e4ee;
            font-family: 'Cascadia Code', 'Consolas', monospace;
        }

        .command-bar {
            padding-top: 2px;
        }

        .command-button {
            font-family: 'Cascadia Code', 'Consolas', monospace;
            font-weight: 700;
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
