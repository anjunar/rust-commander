//! The main window as one composition: titlebar, the two panels over the
//! terminal dock, the busy overlay, the status line and the command bar.
//!
//! The shell owns no behaviour. What the user does goes to [`ShellEvents`];
//! what the application decides comes back through [`ShellModel`] and the
//! handles in [`ShellView`].

use std::{cell::Cell, path::PathBuf, rc::Rc};

use gtk::{gdk, glib, prelude::*};

use crate::{
    application::ActivePanel,
    config::ThemePreference,
    domain::sorting::{SortColumn, SortDirection},
    ui::{
        commands::{bar_command, command, Command, CommandBarMode, COMMANDS},
        dsl::{
            adopt, container, from_notify, hbox, localized_with, paned, render, spinner, text,
            vbox, Ctx, Property, ReadOnly,
        },
        file_panel_view::{file_panel, FilePanel, PanelKeyNavigation},
        main_window_menu::menu_bar,
        terminal_controller::{TerminalAction, TerminalController},
        terminal_dock::{terminal_dock, TerminalDock},
        theme::ThemeController,
    },
};

pub(crate) const APP_WINDOW_TITLE: &str = "RCommander";

/// Pull the divider to the exact centre when a drag comes near the midpoint.
const CENTER_SNAP_IN_PX: i32 = 18;
/// Keep it there until the drag moves farther out again.
const CENTER_SNAP_OUT_PX: i32 = 32;

/// Everything the user can do in the main window, as the shell reports it.
pub(crate) trait ShellEvents {
    fn run_command(&self, command: &'static Command);
    fn panel_focused(&self, panel: ActivePanel);
    fn cursor_changed(&self, panel: ActivePanel, index: usize);
    fn row_activated(&self, panel: ActivePanel, index: usize);
    fn panel_key(&self, panel: ActivePanel, action: PanelKeyNavigation);
    /// Moves the cursor to the first match of `query`; `false` when there is none.
    fn quick_search(&self, panel: ActivePanel, query: &str) -> bool;
    fn tab_activated(&self, panel: ActivePanel, index: usize);
    fn tab_new(&self, panel: ActivePanel);
    fn tab_close(&self, panel: ActivePanel, index: usize);
    fn root_selected(&self, panel: ActivePanel, index: usize);
    fn connect_remote(&self, panel: ActivePanel);
    fn context_menu(
        &self,
        panel: ActivePanel,
        row: Option<usize>,
        anchor: gtk::Widget,
        x: f64,
        y: f64,
    );
    fn sort_changed(&self, panel: ActivePanel, column: SortColumn, direction: SortDirection);
    fn terminal_action(&self, action: TerminalAction);
    fn focus_panels(&self);
}

/// The window-level state the shell is bound to.
pub(crate) struct ShellModel {
    pub status: Property<String>,
    pub busy: Property<bool>,
    pub theme: Property<ThemePreference>,
    pub command_mode: Property<CommandBarMode>,
}

impl ShellModel {
    pub fn new(theme: ThemePreference) -> Self {
        Self {
            status: Property::new(String::new()),
            busy: Property::new(false),
            theme: Property::new(theme),
            command_mode: Property::new(CommandBarMode::Normal),
        }
    }
}

/// Handles the application drives directly.
pub(crate) struct ShellView {
    pub root: Ctx<gtk::ApplicationWindow>,
    pub left: FilePanel,
    pub right: FilePanel,
    /// The split between the two panels.
    pub panes: gtk::Paned,
    /// The split between the panels and the terminal dock.
    pub content: gtk::Paned,
    pub terminal: TerminalDock,
}

impl ShellView {
    pub fn panel(&self, panel: ActivePanel) -> &FilePanel {
        match panel {
            ActivePanel::Left => &self.left,
            ActivePanel::Right => &self.right,
        }
    }
}

/// Composes the main window.
pub(crate) fn build_shell(
    window: &gtk::ApplicationWindow,
    model: &ShellModel,
    terminal: Rc<TerminalController>,
    theme_controller: Rc<ThemeController>,
    events: Rc<dyn ShellEvents>,
) -> ShellView {
    render(window, || {
        let root = adopt(window);

        // Every command is a window action; the command bar points at them.
        for command in COMMANDS {
            let events = Rc::clone(&events);
            let run = move || events.run_command(command);
            if command.toggle {
                root.toggle_action(command.action, command.accels, false, run);
            } else {
                root.action(command.action, command.accels, run);
            }
        }
        track_modifiers(&root, &model.command_mode, &events);

        // The palette follows the preference, and the desktop when it says "System".
        root.add_disposable(
            model
                .theme
                .combine(&prefers_dark(), |preference, _| *preference)
                .observe(move |preference| theme_controller.apply(*preference)),
        );

        #[cfg(not(target_os = "macos"))]
        root.titlebar(|| {
            crate::ui::dsl::header_bar(|header| {
                header.title(|| text(APP_WINDOW_TITLE, |title| title.css("app-title")));
                header.pack_start(menu_bar);
                #[cfg(target_os = "windows")]
                {
                    header.set_show_title_buttons(false);
                    header.pack_end(|| window_controls(window));
                }
            });
        });

        let mut handles = None;
        vbox(8, |shell| {
            shell.margin(8);

            let mut panels = None;
            let mut dock = None;
            let content = paned(gtk::Orientation::Vertical, |content| {
                content.set_hexpand(true);
                content.set_vexpand(true);
                content.set_resize_start_child(true);
                content.set_resize_end_child(false);
                content.set_shrink_start_child(false);
                content.set_shrink_end_child(false);
                panels = Some(content.start(|| commander_panes(&events)));
                dock = Some(content.end(|| terminal_dock(terminal, Rc::clone(&events))));
            });

            let (panes, left, right) = panels.expect("panels composed");
            handles = Some((
                left,
                right,
                panes,
                content.widget().clone(),
                dock.expect("terminal dock composed"),
            ));

            status_line(model);
            command_bar(&model.command_mode);
        });

        let (left, right, panes, content, terminal) = handles.expect("shell composed");
        ShellView {
            root,
            left,
            right,
            panes,
            content,
            terminal,
        }
    })
}

/// The two panels side by side.
fn commander_panes(events: &Rc<dyn ShellEvents>) -> (gtk::Paned, FilePanel, FilePanel) {
    let mut panels = None;
    let panes = paned(gtk::Orientation::Horizontal, |panes| {
        panes.set_hexpand(true);
        panes.set_vexpand(true);
        panes.css("commander-view");
        panes.set_resize_start_child(true);
        panes.set_resize_end_child(true);
        // Panels may shrink below their content's natural width, so changing
        // a folder or a tab never moves the divider away from where it was put.
        panes.set_shrink_start_child(true);
        panes.set_shrink_end_child(true);
        panes.set_wide_handle(false);
        snap_to_center(panes);
        let left = panes.start(|| file_panel(ActivePanel::Left, Rc::clone(events)));
        let right = panes.end(|| file_panel(ActivePanel::Right, Rc::clone(events)));
        panels = Some((left, right));
    });
    let (left, right) = panels.expect("panels composed");
    (panes.widget().clone(), left, right)
}

/// Makes the divider stick to the middle while it is dragged past it.
fn snap_to_center(panes: &Ctx<gtk::Paned>) {
    let snapping = Cell::new(false);
    let latched = Cell::new(false);
    let handler = panes.connect_notify_local(Some("position"), move |panes, _| {
        if snapping.get() {
            return;
        }
        let width = panes.width();
        if width <= 0 {
            return;
        }

        let center = width / 2;
        let position = panes.position();
        let distance = (position - center).abs();
        if latched.get() {
            if distance > CENTER_SNAP_OUT_PX {
                latched.set(false);
                return;
            }
        } else if distance <= CENTER_SNAP_IN_PX {
            latched.set(true);
        } else {
            return;
        }

        if position != center {
            snapping.set(true);
            panes.set_position(center);
            snapping.set(false);
        }
    });
    panes.own_signal(handler);
}

fn status_line(model: &ShellModel) {
    hbox(8, |_| {
        spinner(|spinner| {
            spinner.css("busy-spinner");
            spinner.bind_spinning(&model.busy);
            spinner.bind_visible(&model.busy);
        });
        text(&model.status, |status| {
            status.set_hexpand(true);
            // Paths get long, and a label that may not shrink widens the window.
            status.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
            status.css("status-line");
        });
    });
}

/// The F-key bar: ten buttons that show what their key does under the
/// modifiers held right now.
fn command_bar(mode: &Property<CommandBarMode>) {
    hbox(3, |bar| {
        bar.css("command-bar");
        bar.set_homogeneous(true);
        for key in 1..=10 {
            command_button(key, mode);
        }
    });
}

fn command_button(key: u8, mode: &Property<CommandBarMode>) {
    let label = localized_with(mode, move |mode| {
        bar_command(*mode, key)
            .and_then(|command| command.slot)
            .map(|slot| rust_i18n::t!(slot.label).into_owned())
            .unwrap_or_default()
    });
    let key_text = label.map(|label| split_command_label(label).0);
    let caption = label.map(|label| split_command_label(label).1);

    container(gtk::Button::new(), |button| {
        button.css("command-button");
        button.set_focus_on_click(false);
        button.set_can_focus(false);
        button.css_if("command-button-idle", label.map(String::is_empty));
        // A key with nothing to do in this mode still does its plain job when clicked.
        button.bind(mode, move |button, mode| {
            let command =
                bar_command(*mode, key).or_else(|| bar_command(CommandBarMode::Normal, key));
            button.set_action_name(command.map(Command::detailed_name).as_deref());
        });

        hbox(6, |row| {
            row.set_halign(gtk::Align::Center);
            row.set_valign(gtk::Align::Center);
            row.set_hexpand(true);
            text(&key_text, |label| {
                label.css("command-key");
                label.set_valign(gtk::Align::Center);
                label.bind_visible(key_text.map(|text| !text.is_empty()));
            });
            text(&caption, |label| {
                label.css("command-caption");
                label.set_valign(gtk::Align::Center);
                label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                label.bind_visible(caption.map(|text| !text.is_empty()));
            });
        });
    });
}

/// Splits `F5 Copy` into the key and its caption; either can be empty.
fn split_command_label(full: &str) -> (String, String) {
    let trimmed = full.trim();
    match trimmed.split_once(' ') {
        Some((key, rest)) if is_function_key(key) => (key.to_owned(), rest.trim().to_owned()),
        _ if is_function_key(trimmed) => (trimmed.to_owned(), String::new()),
        _ => (String::new(), trimmed.to_owned()),
    }
}

fn is_function_key(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('F') | Some('f'))
        && chars.next().is_some_and(|ch| ch.is_ascii_digit())
        && chars.all(|ch| ch.is_ascii_digit())
}

/// Follows the modifier keys, for the labels of the command bar, and takes
/// plain F10 before GTK and Windows can turn it into a menu shortcut.
fn track_modifiers(
    root: &Ctx<gtk::ApplicationWindow>,
    mode: &Property<CommandBarMode>,
    events: &Rc<dyn ShellEvents>,
) {
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let mode = mode.clone();
        let events = Rc::clone(events);
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            let modifiers = effective_modifiers(key, modifiers, true);
            mode.set(command_bar_mode(modifiers));

            let any = gdk::ModifierType::ALT_MASK
                | gdk::ModifierType::CONTROL_MASK
                | gdk::ModifierType::SHIFT_MASK
                | gdk::ModifierType::SUPER_MASK;
            if key == gdk::Key::F10 && !modifiers.intersects(any) {
                if let Some(quit) = command("quit") {
                    events.run_command(quit);
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
    }
    {
        let mode = mode.clone();
        keys.connect_key_released(move |_, key, _, modifiers| {
            mode.set(command_bar_mode(effective_modifiers(key, modifiers, false)));
        });
    }
    root.own_controller(keys);

    // A modifier released in another window never reaches this one.
    let mode = mode.clone();
    let handler = root.connect_is_active_notify(move |window| {
        if !window.is_active() {
            mode.set(CommandBarMode::Normal);
        }
    });
    root.own_signal(handler);
}

/// The modifier state including the key that just changed it, which GTK
/// reports only with the next event.
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

fn command_bar_mode(modifiers: gdk::ModifierType) -> CommandBarMode {
    let control = modifiers.contains(gdk::ModifierType::CONTROL_MASK);
    let shift = modifiers.contains(gdk::ModifierType::SHIFT_MASK);
    if control && shift {
        CommandBarMode::Hidden
    } else if control {
        CommandBarMode::Ctrl
    } else if shift {
        CommandBarMode::Shift
    } else if modifiers.contains(gdk::ModifierType::ALT_MASK) {
        CommandBarMode::Alt
    } else {
        CommandBarMode::Normal
    }
}

/// Whether the desktop asks for dark styles; constant where GTK cannot tell.
fn prefers_dark() -> ReadOnly<bool> {
    match gtk::Settings::default() {
        Some(settings) => from_notify(
            &settings,
            "gtk-application-prefer-dark-theme",
            |settings: &gtk::Settings| settings.is_gtk_application_prefer_dark_theme(),
        ),
        None => ReadOnly::constant(false),
    }
}

/// Minimise, maximise/restore and close, for the custom Windows titlebar.
#[cfg(target_os = "windows")]
pub(crate) fn window_controls(window: &gtk::ApplicationWindow) {
    use crate::ui::dsl::icon_button;

    let maximized = from_notify(window, "maximized", |window: &gtk::ApplicationWindow| {
        window.is_maximized()
    });

    hbox(4, |controls| {
        controls.css("window-controls");

        let control = |icon: &str, class: &str, run: Box<dyn Fn()>| {
            icon_button(icon, |button| {
                button.css("window-control-button");
                button.css(class);
                button.css("flat");
                button.set_focus_on_click(false);
                button.set_size_request(36, 24);
                button.on_click(run);
            })
        };

        let minimize = {
            let window = window.clone();
            control(
                "window-minimize-symbolic",
                "window-minimize-button",
                Box::new(move || window.minimize()),
            )
        };
        minimize.set_tooltip_text(Some("Minimize"));

        let maximize = {
            let window = window.clone();
            control(
                "window-maximize-symbolic",
                "window-maximize-button",
                Box::new(move || {
                    if window.is_maximized() {
                        window.unmaximize();
                    } else {
                        window.maximize();
                    }
                }),
            )
        };
        maximize.bind(&maximized, |button, maximized| {
            if *maximized {
                button.set_icon_name("window-restore-symbolic");
                button.set_tooltip_text(Some("Restore"));
            } else {
                button.set_icon_name("window-maximize-symbolic");
                button.set_tooltip_text(Some("Maximize"));
            }
        });

        let close = {
            let window = window.clone();
            control(
                "window-close-symbolic",
                "window-close-button",
                Box::new(move || window.close()),
            )
        };
        close.set_tooltip_text(Some("Close"));
    });
}

/// The directory a fresh terminal starts in when no panel offers one.
pub(crate) fn default_local_directory() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

#[cfg(test)]
#[path = "../../tests/unit/ui_shell_tests.rs"]
mod tests;
