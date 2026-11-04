//! The main window as one composition: titlebar, the two panels over the
//! terminal dock, the busy overlay, the status line and the command bar.
//!
//! The shell owns no behaviour. What the user does goes to [`ShellEvents`];
//! what the application decides comes back through [`ShellModel`] and the
//! handles in [`ShellView`].

use std::{path::PathBuf, rc::Rc};

use gtk::prelude::*;

use crate::{
    application::ActivePanel,
    config::ThemePreference,
    domain::sorting::{SortColumn, SortDirection},
    ui::{
        commands::{Command, COMMANDS},
        dsl::{
            adopt, button, from_notify, hbox, localized, overlay, paned, render, spinner, text,
            vbox, Ctx, Property, ReadOnly,
        },
        file_panel_view::{file_panel, FilePanel},
        terminal_controller::{TerminalAction, TerminalController},
        terminal_dock::{terminal_dock, TerminalDock},
        theme::ThemeController,
    },
};

pub(crate) const APP_WINDOW_TITLE: &str = "RCommander";

/// Everything the user can do in the main window, as the shell reports it.
pub(crate) trait ShellEvents {
    fn run_command(&self, command: &'static Command);
    fn panel_focused(&self, panel: ActivePanel);
    fn selection_changed(&self, panel: ActivePanel, indices: Vec<usize>);
    fn row_activated(&self, panel: ActivePanel, index: usize);
    fn open_requested(&self, panel: ActivePanel);
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
    pub busy_message: Property<String>,
    pub theme: Property<ThemePreference>,
}

impl ShellModel {
    pub fn new(theme: ThemePreference) -> Self {
        Self {
            status: Property::new(String::new()),
            busy: Property::new(false),
            busy_message: Property::new(String::new()),
            theme: Property::new(theme),
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
            root.action(command.action, command.accels, move || {
                events.run_command(command)
            });
        }

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

            overlay(|stage| {
                stage.set_hexpand(true);
                stage.set_vexpand(true);
                let mut panels = None;
                let mut dock = None;
                let content = paned(gtk::Orientation::Vertical, |content| {
                    content.set_hexpand(true);
                    content.set_vexpand(true);
                    content.set_resize_start_child(true);
                    content.set_resize_end_child(false);
                    content.set_shrink_start_child(false);
                    content.set_shrink_end_child(false);
                    content.bind_sensitive(model.busy.map(|busy| !busy));
                    panels = Some(content.start(|| commander_panes(&events)));
                    dock = Some(content.end(|| terminal_dock(terminal, Rc::clone(&events))));
                });
                stage.layer(|| busy_overlay(model));

                let (panes, left, right) = panels.expect("panels composed");
                handles = Some((
                    left,
                    right,
                    panes,
                    content.widget().clone(),
                    dock.expect("terminal dock composed"),
                ));
            });

            status_line(model);
            command_bar();
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
        panes.set_shrink_start_child(false);
        panes.set_shrink_end_child(false);
        panes.set_wide_handle(true);
        let left = panes.start(|| file_panel(ActivePanel::Left, Rc::clone(events)));
        let right = panes.end(|| file_panel(ActivePanel::Right, Rc::clone(events)));
        panels = Some((left, right));
    });
    let (left, right) = panels.expect("panels composed");
    (panes.widget().clone(), left, right)
}

/// A centred card shown while a directory loads; it lets clicks through.
fn busy_overlay(model: &ShellModel) {
    vbox(12, |card| {
        card.set_halign(gtk::Align::Center);
        card.set_valign(gtk::Align::Center);
        card.css("navigation-overlay");
        card.set_can_target(false);
        card.bind(&model.busy, |card, busy| {
            card.set_opacity(if *busy { 1.0 } else { 0.0 });
            card.set_sensitive(*busy);
        });
        spinner(|spinner| {
            spinner.css("navigation-overlay-spinner");
            spinner.bind_spinning(&model.busy);
        });
        text(&model.busy_message, |label| {
            label.set_xalign(0.5);
            label.css("navigation-overlay-label");
        });
    });
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
            status.css("status-line");
        });
    });
}

/// The F-key bar: one button per labelled command, bound to its action.
fn command_bar() {
    hbox(6, |bar| {
        bar.css("command-bar");
        bar.set_homogeneous(true);
        for command in COMMANDS {
            let Some(label) = command.label else {
                continue;
            };
            button(
                localized(move || rust_i18n::t!(label).into_owned()),
                |button| {
                    button.css("command-button");
                    button.set_action_name(Some(&command.detailed_name()));
                },
            );
        }
    });
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
fn window_controls(window: &gtk::ApplicationWindow) {
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
                button.set_size_request(44, 28);
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
