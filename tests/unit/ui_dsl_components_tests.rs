use std::{cell::Cell, rc::Rc};

use gtk::prelude::*;

use crate::ui::dsl::{
    adopt, button, check, column_view, dropdown, entry, foreach, from_notify, header_bar, icon,
    mount, overlay, paned, password, popup_menu, progress, relocalize, render, sorter_by, spin,
    spinner, switch, text, tr, vbox, when, Disposable, ListProperty, Property,
};

/// One test for everything that touches GTK: widgets belong to the thread
/// that initialised GTK, and the test harness gives every test its own thread.
/// Without a display (headless CI) the test skips instead of failing.
#[test]
fn dsl_drives_gtk_widgets_and_releases_its_scope() {
    if gtk::init().is_err() {
        eprintln!("skipping GTK DSL test: no display available");
        return;
    }

    bindings_follow_properties_and_the_scope_ends_with_the_window();
    controls_bind_both_ways_and_slots_place_children();
    foreach_keeps_children_in_step_with_the_list();
    window_blocks_actions_titlebar_and_timers();
    column_view_declares_typed_columns();
    popup_menu_items_close_the_menu_and_run();

    // Application views last: they queue idle callbacks (presenting dialogs)
    // that must not run, so nothing iterates the main loop after them.
    shell_composes_the_main_window();
    viewer_and_editor_open_files();
}

// ---------------------------------------------------------------------------
// Application views built with the DSL
// ---------------------------------------------------------------------------

#[derive(Default)]
struct RecordingEvents {
    log: std::cell::RefCell<Vec<String>>,
}

impl RecordingEvents {
    fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.log.borrow_mut())
    }

    fn record(&self, event: String) {
        self.log.borrow_mut().push(event);
    }
}

impl crate::ui::shell::ShellEvents for RecordingEvents {
    fn run_command(&self, command: &'static crate::ui::commands::Command) {
        self.record(format!("command {}", command.action));
    }
    fn panel_focused(&self, panel: crate::application::ActivePanel) {
        self.record(format!("focus {panel:?}"));
    }
    fn selection_changed(&self, panel: crate::application::ActivePanel, indices: Vec<usize>) {
        self.record(format!("select {panel:?} {indices:?}"));
    }
    fn row_activated(&self, panel: crate::application::ActivePanel, index: usize) {
        self.record(format!("activate {panel:?} {index}"));
    }
    fn open_requested(&self, panel: crate::application::ActivePanel) {
        self.record(format!("open {panel:?}"));
    }
    fn root_selected(&self, panel: crate::application::ActivePanel, index: usize) {
        self.record(format!("root {panel:?} {index}"));
    }
    fn connect_remote(&self, panel: crate::application::ActivePanel) {
        self.record(format!("remote {panel:?}"));
    }
    fn context_menu(
        &self,
        panel: crate::application::ActivePanel,
        row: Option<usize>,
        _anchor: gtk::Widget,
        _x: f64,
        _y: f64,
    ) {
        self.record(format!("menu {panel:?} {row:?}"));
    }
    fn sort_changed(
        &self,
        panel: crate::application::ActivePanel,
        column: crate::domain::sorting::SortColumn,
        direction: crate::domain::sorting::SortDirection,
    ) {
        self.record(format!("sort {panel:?} {column:?} {direction:?}"));
    }
    fn terminal_action(&self, action: crate::ui::terminal_controller::TerminalAction) {
        self.record(format!("terminal {action:?}"));
    }
    fn focus_panels(&self) {
        self.record("focus panels".into());
    }
}

/// Every widget of type `T` below `root`, depth first.
fn descendants<T: IsA<gtk::Widget>>(root: &impl IsA<gtk::Widget>) -> Vec<T> {
    let mut found = Vec::new();
    let mut stack = vec![root.as_ref().clone()];
    while let Some(widget) = stack.pop() {
        let mut child = widget.last_child();
        while let Some(current) = child {
            child = current.prev_sibling();
            stack.push(current);
        }
        if let Ok(typed) = widget.downcast::<T>() {
            found.push(typed);
        }
    }
    found
}

fn with_class<T: IsA<gtk::Widget>>(root: &impl IsA<gtk::Widget>, class: &str) -> Vec<T> {
    descendants::<T>(root)
        .into_iter()
        .filter(|widget| widget.has_css_class(class))
        .collect()
}

fn shell_composes_the_main_window() {
    use crate::{
        config::ThemePreference,
        domain::{Entry, EntryKind, PanelLocation, RootLocation},
        ui::{
            commands::COMMANDS,
            shell::{build_shell, ShellModel},
            terminal_controller::TerminalController,
            theme::ThemeController,
        },
    };

    let window = gtk::ApplicationWindow::builder().build();
    let model = ShellModel::new(ThemePreference::Dark);
    let events = Rc::new(RecordingEvents::default());
    let view = build_shell(
        &window,
        &model,
        Rc::new(TerminalController::new(std::env::temp_dir())),
        Rc::new(ThemeController::new()),
        events.clone(),
    );

    // Every command is a window action; the labelled ones have a bar button
    // pointing at it, so button and shortcut run the same command.
    for command in COMMANDS {
        assert!(
            window.lookup_action(command.action).is_some(),
            "{}",
            command.action
        );
    }
    let bar = with_class::<gtk::Button>(&window, "command-button");
    assert_eq!(
        bar.len(),
        COMMANDS.iter().filter(|c| c.label.is_some()).count()
    );
    assert_eq!(bar[4].action_name().as_deref(), Some("win.copy"));
    assert!(bar
        .iter()
        .all(|button| button.label().is_some_and(|label| !label.is_empty())));
    WidgetExt::activate_action(&window, "win.copy", None).expect("copy action");
    assert_eq!(events.take(), vec!["command copy".to_string()]);

    // Status line and busy state follow the model.
    model.status.set("Ready".into());
    let status = with_class::<gtk::Label>(&window, "status-line");
    assert_eq!(status[0].label().as_str(), "Ready");
    model.busy.set(true);
    assert!(!view.content.is_sensitive());
    model.busy.set(false);
    assert!(view.content.is_sensitive());

    // Applying state to a panel is not user input.
    let location = PanelLocation::filesystem(std::env::temp_dir());
    view.left.set_roots(
        &[
            RootLocation {
                label: "C:".into(),
                path: "C:\\".into(),
            },
            RootLocation {
                label: "D:".into(),
                path: "D:\\".into(),
            },
        ],
        Some(0),
    );
    view.left.set_path("C:\\temp");
    let file = Entry {
        name: "a.txt".into(),
        archive_path: None,
        remote_path: None,
        kind: EntryKind::File,
        is_dir: false,
        size_bytes: 3,
        modified_at: None,
        attributes: String::new(),
        is_parent_link: false,
    };
    view.left.set_entries(
        &location,
        &[Entry::parent_link(), file],
        [1].into_iter().collect(),
    );
    view.left.set_active(true);
    assert!(events.take().is_empty(), "applying state reported events");

    let pickers = with_class::<gtk::DropDown>(&window, "root-selector");
    let left_picker = &pickers[0];
    assert_eq!(left_picker.selected(), 0);
    assert_eq!(left_picker.model().expect("roots model").n_items(), 2);
    let paths = with_class::<gtk::Label>(&window, "path-label");
    assert_eq!(paths[0].label().as_str(), "C:\\temp");
    let left_panel = with_class::<gtk::Box>(&window, "file-panel-left");
    assert!(left_panel[0].has_css_class("active-panel"));
    let tables = with_class::<gtk::ColumnView>(&window, "file-table");
    assert_eq!(tables.len(), 2);
    assert_eq!(tables[0].columns().n_items(), 5);
    assert_eq!(tables[0].model().expect("selection").n_items(), 2);

    // A user choice is reported.
    left_picker.set_selected(1);
    assert_eq!(events.take(), vec!["root Left 1".to_string()]);

    // The dock follows the terminal state. Only the placeholder backend is
    // toggled here; on Linux toggling would start a real shell.
    assert!(!view.terminal.is_visible());
    let dock = with_class::<gtk::Box>(&window, "terminal-dock");
    assert!(!dock[0].get_visible());
    if !view.terminal.is_supported() {
        view.terminal.toggle();
        assert!(view.terminal.is_visible());
        assert!(dock[0].get_visible());
    }

    WidgetExt::realize(&window);
    window.destroy();
}

fn viewer_and_editor_open_files() {
    use std::fs;

    let directory = std::env::temp_dir().join(format!("rcommander-ui-test-{}", std::process::id()));
    fs::create_dir_all(&directory).expect("test directory");
    let file = directory.join("notes.txt");
    fs::write(&file, "first line\nsecond line\n").expect("test file");
    let parent = gtk::ApplicationWindow::builder().build();

    let dialog_of = |parent: &gtk::ApplicationWindow| -> gtk::Window {
        let toplevels = gtk::Window::list_toplevels();
        toplevels
            .into_iter()
            .filter_map(|window| window.downcast::<gtk::Window>().ok())
            .find(|window| window.transient_for().as_ref() == Some(parent.upcast_ref()))
            .expect("dialog over the parent")
    };

    // The viewer renders the file into its text view right away.
    crate::ui::file_viewer_dialog::open(&parent, file.clone(), Default::default())
        .expect("viewer opens");
    let viewer = dialog_of(&parent);
    let text_view = descendants::<gtk::TextView>(&viewer)
        .into_iter()
        .next()
        .expect("viewer text");
    let buffer = text_view.buffer();
    let shown = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
    assert!(shown.contains("first line"), "viewer shows: {shown}");
    WidgetExt::realize(&viewer);
    viewer.destroy();

    // The editor loads the file and saves what the user typed.
    let saved = Rc::new(Cell::new(false));
    {
        let saved = Rc::clone(&saved);
        crate::ui::editor_dialog::edit_file(&parent, file.clone(), move |_| saved.set(true))
            .expect("editor opens");
    }
    let editor = dialog_of(&parent);
    let source = descendants::<sourceview5::View>(&editor)
        .into_iter()
        .next()
        .expect("editor view");
    let buffer = source.buffer();
    assert_eq!(
        buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .as_str(),
        "first line\nsecond line\n"
    );
    buffer.set_text("changed\n");
    let save = with_class::<gtk::Button>(&editor, "suggested-action");
    WidgetExt::realize(&editor);
    save[0].emit_clicked();
    assert!(saved.get());
    assert_eq!(fs::read_to_string(&file).expect("saved file"), "changed\n");

    fs::remove_dir_all(&directory).expect("test directory removed");
}

fn window_blocks_actions_titlebar_and_timers() {
    let runs = Rc::new(Cell::new(0));
    let ticks = Rc::new(Cell::new(0));
    let fired = Rc::new(Cell::new(false));
    let late = Rc::new(Cell::new(false));
    let caption = gtk::Label::new(Some("before"));
    let mut seen_caption = None;
    let mut late_label = None;

    let window = gtk::ApplicationWindow::builder().build();
    render(&window, || {
        let root = adopt(&window);
        {
            let runs = Rc::clone(&runs);
            root.action("count", &[], move || runs.set(runs.get() + 1));
        }
        root.titlebar(|| {
            header_bar(|h| {
                h.title(|| text(tr!("common.close"), |_| {}));
            })
        });

        let content = vbox(0, |content| {
            let follows = from_notify(&caption, "label", |label: &gtk::Label| {
                label.label().to_string()
            });
            seen_caption = Some(text(follows, |_| {}));
            button("Run", |b| b.set_action_name(Some("win.count")));
            {
                let ticks = Rc::clone(&ticks);
                content.every(std::time::Duration::from_millis(1), move || {
                    ticks.set(ticks.get() + 1)
                });
            }
            {
                let fired = Rc::clone(&fired);
                content.after(std::time::Duration::from_millis(1), move || fired.set(true));
            }
            {
                let late = Rc::clone(&late);
                content.after(std::time::Duration::from_secs(3600), move || late.set(true));
            }
        });
        // Composing later adds to the same container and scope.
        late_label = Some(content.compose(|| text("added later", |_| {})));
    });
    let seen_caption = seen_caption.expect("caption composed");
    let late_label = late_label.expect("late label composed");

    // Actions run by name; buttons pointing at them share the action.
    WidgetExt::activate_action(&window, "win.count", None).expect("action exists");
    assert_eq!(runs.get(), 1);

    // The titlebar slot holds the header bar with its localized title.
    let header = window
        .titlebar()
        .and_then(|titlebar| titlebar.downcast::<gtk::HeaderBar>().ok())
        .expect("header bar in the titlebar slot");
    let title = header
        .title_widget()
        .and_then(|title| title.downcast::<gtk::Label>().ok())
        .expect("title label");
    assert!(!title.label().is_empty());
    relocalize();
    assert!(!title.label().is_empty());

    // GObject properties are observable values.
    assert_eq!(seen_caption.label().as_str(), "before");
    caption.set_label("after");
    assert_eq!(seen_caption.label().as_str(), "after");

    assert!(late_label.parent().is_some());

    // Timers run on the main loop and end with the scope.
    let context = gtk::glib::MainContext::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while (!fired.get() || ticks.get() < 2) && std::time::Instant::now() < deadline {
        context.iteration(false);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(fired.get());
    assert!(ticks.get() >= 2);

    // Hiding keeps the bindings; destroying a realized window ends them.
    window.set_visible(false);
    assert!(window.lookup_action("count").is_some());
    WidgetExt::realize(&window);
    window.destroy();
    assert!(window.lookup_action("count").is_none());
    let ticks_after_destroy = ticks.get();
    for _ in 0..20 {
        context.iteration(false);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(ticks.get(), ticks_after_destroy);
    assert!(!late.get());
    caption.set_label("ignored");
    assert_eq!(seen_caption.label().as_str(), "after");
}

fn column_view_declares_typed_columns() {
    let store = gtk::StringList::new(&["b", "a"]);
    let selection = gtk::SingleSelection::new(Some(store));
    let mut table = None;

    let window = gtk::Window::new();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    window.set_child(Some(&content));
    render(&content, || {
        table = Some(column_view(&selection, |table| {
            let name = table.column(
                tr!("column.name"),
                || text("", |_| {}).widget().clone(),
                |label: &gtk::Label, row: &gtk::StringObject| label.set_label(&row.string()),
            );
            name.set_sorter(Some(&sorter_by(
                |first: &gtk::StringObject, second: &gtk::StringObject| {
                    first.string().cmp(&second.string())
                },
            )));
            table.column(
                "Length",
                || text("", |_| {}).widget().clone(),
                |label: &gtk::Label, row: &gtk::StringObject| {
                    label.set_label(&row.string().len().to_string())
                },
            );
        }));
    });
    let table = table.expect("column view composed");

    let columns = table.columns();
    assert_eq!(columns.n_items(), 2);
    let first = columns
        .item(0)
        .and_then(|column| column.downcast::<gtk::ColumnViewColumn>().ok())
        .expect("first column");
    assert!(first.title().is_some_and(|title| !title.is_empty()));
    assert!(first.sorter().is_some());
    // Nothing is laid out yet, so no point hits a row.
    assert_eq!(table.row_at(1.0, 1.0), None);

    WidgetExt::realize(&content);
    window.destroy();
}

fn popup_menu_items_close_the_menu_and_run() {
    let opened = Rc::new(Cell::new(0));
    let deleted = Rc::new(Cell::new(0));
    let mut first = None;

    let window = gtk::Window::new();
    let anchor = gtk::Box::new(gtk::Orientation::Vertical, 0);
    window.set_child(Some(&anchor));
    let popover = popup_menu(&anchor, 10.0, 10.0, |menu| {
        let opened = Rc::clone(&opened);
        first = Some(menu.item("Open", move || opened.set(opened.get() + 1)));
        menu.separator();
        let deleted = Rc::clone(&deleted);
        menu.destructive("Delete", move || deleted.set(deleted.get() + 1));
    });
    let first = first.expect("menu item composed");

    assert_eq!(
        popover.parent().as_ref(),
        Some(anchor.upcast_ref::<gtk::Widget>())
    );
    let content = popover
        .child()
        .and_then(|child| child.downcast::<gtk::Box>().ok())
        .expect("menu content");
    let mut children = 0;
    let mut child = content.first_child();
    while let Some(current) = child {
        children += 1;
        child = current.next_sibling();
    }
    assert_eq!(children, 3);
    let last = content.last_child().expect("last item");
    assert!(last.has_css_class("destructive-action"));

    first.emit_clicked();
    assert_eq!(opened.get(), 1);
    assert_eq!(deleted.get(), 0);

    popover.unparent();
    window.destroy();
}

fn bindings_follow_properties_and_the_scope_ends_with_the_window() {
    let count = Property::new(0);
    let flag = Property::new(false);
    let name = Property::new("a".to_string());
    let selected = Property::new(1u32);
    let number = Property::new(5.0);
    let shown = Property::new(false);
    let released = Rc::new(Cell::new(false));
    let inner_released = Rc::new(Cell::new(false));

    let mut label = None;
    let mut increment = None;
    let mut toggle = None;
    let mut field = None;
    let mut choice = None;
    let mut spinner = None;
    let mut anchor = None;

    // Same topology as the dialogs: the render host is a box inside the window.
    let window = gtk::Window::new();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    window.set_child(Some(&content));
    render(&content, || {
        vbox(4, |root| {
            let released = Rc::clone(&released);
            root.add_disposable(Disposable::new(move || released.set(true)));

            label = Some(text(count.map(|value| format!("Count: {value}")), |_| {}));
            increment = Some(button("Increment", |b| {
                let count = count.clone();
                b.on_click(move || count.update(|value| value + 1));
            }));
            toggle = Some(switch(&flag, |_| {}));
            field = Some(entry(&name, |_| {}));
            choice = Some(dropdown(&["x", "y", "z"], &selected, |_| {}));
            spinner = Some(spin(&number, 0.0, 10.0, 1.0, |_| {}));

            let inner_released = Rc::clone(&inner_released);
            anchor = Some(when(&shown, move || {
                let inner_released = Rc::clone(&inner_released);
                mount(gtk::Label::new(Some("inner")), |inner| {
                    inner.add_disposable(Disposable::new(move || inner_released.set(true)));
                });
            }));
        });
    });

    let label = label.expect("label composed");
    let increment = increment.expect("button composed");
    let toggle = toggle.expect("switch composed");
    let field = field.expect("entry composed");
    let choice = choice.expect("dropdown composed");
    let spinner = spinner.expect("spin button composed");
    let anchor = anchor.expect("condition composed");

    // Reactive text.
    assert_eq!(label.label().as_str(), "Count: 0");
    count.set(1);
    assert_eq!(label.label().as_str(), "Count: 1");

    // Click handler.
    increment.emit_clicked();
    assert_eq!(count.get(), 2);
    assert_eq!(label.label().as_str(), "Count: 2");

    // Two-way bindings: property to widget, then widget to property.
    assert!(!toggle.is_active());
    flag.set(true);
    assert!(toggle.is_active());
    toggle.set_active(false);
    assert!(!flag.get());

    assert_eq!(field.text().as_str(), "a");
    name.set("b".to_string());
    assert_eq!(field.text().as_str(), "b");
    field.set_text("c");
    assert_eq!(name.get(), "c");

    assert_eq!(choice.selected(), 1);
    selected.set(2);
    assert_eq!(choice.selected(), 2);
    choice.set_selected(0);
    assert_eq!(selected.get(), 0);

    assert_eq!(spinner.value(), 5.0);
    number.set(7.0);
    assert_eq!(spinner.value(), 7.0);
    spinner.set_value(3.0);
    assert_eq!(number.get(), 3.0);

    // Conditional composition with a scope of its own. `get_visible` is the
    // widget's own flag; `is_visible` would also require the unshown window.
    assert!(anchor.first_child().is_none());
    assert!(!anchor.get_visible());
    shown.set(true);
    assert!(anchor.first_child().is_some());
    assert!(anchor.get_visible());
    assert!(!inner_released.get());
    shown.set(false);
    assert!(anchor.first_child().is_none());
    assert!(inner_released.get());

    // Destroying the window unrealizes the host: the scope is released, the
    // click handler is disconnected and the text subscription is gone.
    WidgetExt::realize(&content);
    assert!(!released.get());
    window.destroy();
    assert!(released.get());
    increment.emit_clicked();
    assert_eq!(count.get(), 2);
    count.set(9);
    assert_eq!(label.label().as_str(), "Count: 2");
}

fn controls_bind_both_ways_and_slots_place_children() {
    let secret = Property::new("s".to_string());
    let agreed = Property::new(false);
    let busy = Property::new(true);
    let fraction = Property::new(0.25);
    let close_requests = Rc::new(Cell::new(0));
    let released = Rc::new(Cell::new(false));

    let mut secret_entry = None;
    let mut agree = None;
    let mut wheel = None;
    let mut bar = None;
    let mut split = None;
    let mut layers = None;
    let mut header = None;
    let mut menu = None;

    let window = gtk::Window::new();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    window.set_child(Some(&content));
    render(&content, || {
        vbox(0, |root| {
            let released = Rc::clone(&released);
            root.add_disposable(Disposable::new(move || released.set(true)));

            secret_entry = Some(password(&secret, |_| {}));
            agree = Some(check("Agree", &agreed, |_| {}));
            wheel = Some(spinner(|s| s.bind_spinning(&busy)));
            bar = Some(progress(&fraction, |p| {
                p.bind_progress_text(fraction.map(|value| format!("{:.0}%", value * 100.0)));
            }));
            split = Some(paned(gtk::Orientation::Horizontal, |p| {
                p.start(|| text("left", |_| {}));
                p.end(|| text("right", |_| {}));
            }));
            layers = Some(overlay(|o| {
                text("base", |_| {});
                o.layer(|| icon("dialog-information-symbolic", |_| {}));
            }));
            header = Some(header_bar(|h| {
                h.pack_start(|| button("Back", |_| {}));
                h.title(|| text("Title", |_| {}));
                h.pack_end(|| button("More", |_| {}));
            }));
            let trigger = button("Open menu", |_| {});
            menu = Some(trigger.popover(|_| {
                text("item", |_| {});
            }));
        });

        // An existing widget joins the scope through `adopt`.
        let close_requests = Rc::clone(&close_requests);
        adopt(&window).on_close_request(move || {
            close_requests.set(close_requests.get() + 1);
            true
        });
    });

    let secret_entry = secret_entry.expect("password composed");
    let agree = agree.expect("check composed");
    let wheel = wheel.expect("spinner composed");
    let bar = bar.expect("progress composed");
    let split = split.expect("paned composed");
    let layers = layers.expect("overlay composed");
    let header = header.expect("header bar composed");
    let menu = menu.expect("popover composed");

    assert_eq!(secret_entry.text().as_str(), "s");
    secret.set("t".to_string());
    assert_eq!(secret_entry.text().as_str(), "t");
    secret_entry.set_text("u");
    assert_eq!(secret.get(), "u");

    assert!(!agree.is_active());
    agreed.set(true);
    assert!(agree.is_active());
    agree.set_active(false);
    assert!(!agreed.get());

    assert!(wheel.is_spinning());
    busy.set(false);
    assert!(!wheel.is_spinning());

    assert_eq!(bar.fraction(), 0.25);
    assert_eq!(bar.text().as_deref(), Some("25%"));
    fraction.set(0.5);
    assert_eq!(bar.fraction(), 0.5);
    assert_eq!(bar.text().as_deref(), Some("50%"));

    assert!(split.start_child().is_some());
    assert!(split.end_child().is_some());
    assert!(layers.child().is_some());
    assert!(layers
        .last_child()
        .and_then(|child| child.downcast::<gtk::Image>().ok())
        .is_some());
    assert!(header.title_widget().is_some());
    assert!(menu.parent().is_some());

    // A close request the handler refuses keeps the window and its scope.
    WidgetExt::realize(&content);
    window.close();
    assert_eq!(close_requests.get(), 1);
    assert!(!released.get());

    // Destroying it releases the scope, which also unparents the popover.
    window.destroy();
    assert!(released.get());
    assert!(menu.parent().is_none());
}

fn foreach_keeps_children_in_step_with_the_list() {
    let names = ListProperty::new(vec!["a".to_string(), "b".to_string()]);
    let released = Rc::new(Cell::new(0));
    let mut rows = None;

    let window = gtk::Window::new();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    window.set_child(Some(&content));
    render(&content, || {
        let released = Rc::clone(&released);
        rows = Some(foreach(&names, move |name, index| {
            let released = Rc::clone(&released);
            text(format!("{index}:{name}"), |label| {
                label.add_disposable(Disposable::new(move || released.set(released.get() + 1)));
            });
        }));
    });
    let rows = rows.expect("foreach composed");

    assert_eq!(labels(&rows), vec!["0:a", "1:b"]);

    names.push("c".to_string());
    assert_eq!(labels(&rows), vec!["0:a", "1:b", "2:c"]);

    // The index is the position at composition time; siblings keep theirs.
    names.insert(1, "x".to_string());
    assert_eq!(labels(&rows), vec!["0:a", "1:x", "1:b", "2:c"]);

    names.remove(0);
    assert_eq!(labels(&rows), vec!["1:x", "1:b", "2:c"]);
    assert_eq!(released.get(), 1);

    names.set(0, "y".to_string());
    assert_eq!(labels(&rows), vec!["0:y", "1:b", "2:c"]);
    assert_eq!(released.get(), 2);

    names.clear();
    assert!(labels(&rows).is_empty());
    assert_eq!(released.get(), 5);

    names.replace_all(vec!["z".to_string()]);
    assert_eq!(labels(&rows), vec!["0:z"]);

    WidgetExt::realize(&content);
    window.destroy();
    assert_eq!(released.get(), 6);
}

fn labels(rows: &gtk::Box) -> Vec<String> {
    let mut texts = Vec::new();
    let mut wrapper = rows.first_child();
    while let Some(current) = wrapper {
        if let Some(label) = current
            .first_child()
            .and_then(|child| child.downcast::<gtk::Label>().ok())
        {
            texts.push(label.label().to_string());
        }
        wrapper = current.next_sibling();
    }
    texts
}
