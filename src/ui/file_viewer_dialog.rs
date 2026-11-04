use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    time::Duration,
};

use anyhow::Result;
use gtk::{gdk, prelude::*};
use rust_i18n::t;

use crate::{
    config::ViewerConfig,
    ui::{
        dialogs::{modal, present_and_focus},
        dsl::{frame, hbox, scrollbar, text, text_view, tr, Property},
    },
    viewer::ViewerState,
};

/// Opens `path` in the streaming viewer.
///
/// The viewer renders a window of lines itself instead of loading the file
/// into the text view, so the scrollbars drive [`ViewerState`] and the text
/// view only shows what it rendered.
pub fn open(parent: &gtk::ApplicationWindow, path: PathBuf, config: ViewerConfig) -> Result<()> {
    let viewer = Rc::new(Viewer::open(&path, &config)?);

    let (window, view) = modal(parent, &t!("viewer.window_title"), (980, 720), |dialog| {
        let window = dialog.window();
        window.bind(&viewer.title, |window, title| {
            if !title.is_empty() {
                window.set_title(Some(title.as_str()));
            }
        });
        {
            let viewer = Rc::clone(&viewer);
            let close = window.widget().clone();
            window.on_key_pressed(move |key, _| viewer.handle_key(key, &close));
        }
        {
            let follower = Rc::clone(&viewer);
            let handler = viewer
                .vertical
                .connect_value_changed(move |_| follower.follow_vertical_scrollbar());
            window.own_object_signal(&viewer.vertical, handler);
        }
        {
            let follower = Rc::clone(&viewer);
            let handler = viewer
                .horizontal
                .connect_value_changed(move |_| follower.follow_horizontal_scrollbar());
            window.own_object_signal(&viewer.horizontal, handler);
        }

        let view = dialog.content(|| {
            text(path.display().to_string(), |label| label.css("path-label"));

            let mut view = None;
            hbox(0, |area| {
                area.set_hexpand(true);
                area.set_vexpand(true);
                frame(None, |frame| {
                    frame.set_hexpand(true);
                    frame.set_vexpand(true);
                    frame.css("panel-scroller");
                    view = Some(text_view(&gtk::TextBuffer::new(None), |view| {
                        view.set_editable(false);
                        view.set_cursor_visible(false);
                        view.set_monospace(true);
                        view.set_wrap_mode(if config.line_wrap {
                            gtk::WrapMode::WordChar
                        } else {
                            gtk::WrapMode::None
                        });
                        view.set_hexpand(true);
                        view.set_vexpand(true);
                        view.css("editor-view");
                        view.set_left_margin(8);
                        view.set_top_margin(8);
                        view.set_bottom_margin(8);
                        view.bind(&viewer.body, |view, body| view.buffer().set_text(body));

                        let scroller = Rc::clone(&viewer);
                        view.on_scroll(
                            gtk::EventControllerScrollFlags::HORIZONTAL
                                | gtk::EventControllerScrollFlags::DISCRETE,
                            move |dx, _| scroller.scroll_horizontally(dx),
                        );
                        let scroller = Rc::clone(&viewer);
                        view.on_scroll(gtk::EventControllerScrollFlags::VERTICAL, move |_, dy| {
                            scroller.scroll_vertically(dy)
                        });
                    }));
                });
                scrollbar(gtk::Orientation::Vertical, &viewer.vertical, |_| {});
            });
            scrollbar(gtk::Orientation::Horizontal, &viewer.horizontal, |_| {});
            text(&viewer.status, |status| {
                status.set_hexpand(true);
                status.css("editor-status");
            });

            view.expect("viewer text composed")
        });

        let close = dialog.actions(|| {
            let close = dialog.close_button(tr!("common.close"));
            close.css("command-button");
            close
        });
        window.set_default_widget(Some(close.widget()));

        // Once laid out, the view knows how many lines fit.
        {
            let viewer = Rc::clone(&viewer);
            let view = view.widget().clone();
            window.after(Duration::from_millis(60), move || {
                viewer.set_visible_lines(estimate_visible_lines(&view));
            });
        }

        view
    });

    viewer.refresh();
    present_and_focus(&window, view.widget());
    Ok(())
}

/// The viewer's state and what it shows; the widgets bind to the properties.
struct Viewer {
    state: RefCell<ViewerState>,
    title: Property<String>,
    body: Property<String>,
    status: Property<String>,
    vertical: gtk::Adjustment,
    horizontal: gtk::Adjustment,
    /// Set while the scrollbars are moved to match the state, so their
    /// change signals are not taken for user scrolling.
    syncing: Cell<bool>,
}

impl Viewer {
    fn open(path: &std::path::Path, config: &ViewerConfig) -> Result<Self> {
        Ok(Self {
            state: RefCell::new(ViewerState::open(path, config)?),
            title: Property::new(String::new()),
            body: Property::new(String::new()),
            status: Property::new(String::new()),
            vertical: gtk::Adjustment::new(0.0, 0.0, 1.0, 1.0, 1.0, 1.0),
            horizontal: gtk::Adjustment::new(0.0, 0.0, 4096.0, 8.0, 64.0, 120.0),
            syncing: Cell::new(false),
        })
    }

    /// Renders the visible window of the file and moves the scrollbars to match.
    fn refresh(&self) {
        let mut state = self.state.borrow_mut();
        match state.render() {
            Ok(rendered) => {
                self.title.set(rendered.title);
                self.body.set(rendered.body);
                self.status.set(rendered.status);
                self.sync_scrollbars(&state);
            }
            Err(error) => {
                self.body
                    .set(t!("viewer.render_error_body", error = error.to_string()).into_owned());
                self.status
                    .set(t!("viewer.error_with_detail", error = error.to_string()).into_owned());
            }
        }
    }

    fn change(&self, change: impl FnOnce(&mut ViewerState)) {
        change(&mut self.state.borrow_mut());
        self.refresh();
    }

    fn handle_key(&self, key: gdk::Key, window: &gtk::Window) -> bool {
        match key {
            gdk::Key::Escape => window.close(),
            gdk::Key::Up => self.change(ViewerState::scroll_line_up),
            gdk::Key::Down => self.change(ViewerState::scroll_line_down),
            gdk::Key::Page_Up => self.change(ViewerState::page_up),
            gdk::Key::Page_Down => self.change(ViewerState::page_down),
            gdk::Key::Home => self.change(ViewerState::go_to_start),
            gdk::Key::End => {
                let result = self.state.borrow_mut().go_to_end();
                match result {
                    Ok(()) => self.refresh(),
                    Err(error) => self.status.set(
                        t!("viewer.error_with_detail", error = error.to_string()).into_owned(),
                    ),
                }
            }
            gdk::Key::Left => self.change(ViewerState::scroll_left),
            gdk::Key::Right => self.change(ViewerState::scroll_right),
            gdk::Key::F2 => self.change(ViewerState::toggle_hex_mode),
            _ => return false,
        }
        true
    }

    fn scroll_vertically(&self, dy: f64) -> bool {
        if dy > 0.0 {
            self.change(ViewerState::scroll_line_down);
        } else if dy < 0.0 {
            self.change(ViewerState::scroll_line_up);
        } else {
            return false;
        }
        true
    }

    fn scroll_horizontally(&self, dx: f64) -> bool {
        if dx > 0.0 {
            self.change(ViewerState::scroll_right);
        } else if dx < 0.0 {
            self.change(ViewerState::scroll_left);
        } else {
            return false;
        }
        true
    }

    fn follow_vertical_scrollbar(&self) {
        if self.syncing.get() {
            return;
        }
        let target_line = self.vertical.value().round().max(0.0) as usize;
        let changed = {
            let mut state = self.state.borrow_mut();
            if target_line == state.first_visible_line() {
                false
            } else if state.set_first_visible_line(target_line).is_ok() {
                true
            } else {
                self.status
                    .set(t!("viewer.scroll_update_error").into_owned());
                false
            }
        };
        if changed {
            self.refresh();
        }
    }

    fn follow_horizontal_scrollbar(&self) {
        if self.syncing.get() {
            return;
        }
        let target_column = self.horizontal.value().round().max(0.0) as usize;
        let changed = {
            let mut state = self.state.borrow_mut();
            if target_column == state.horizontal_offset() {
                false
            } else {
                while state.horizontal_offset() < target_column {
                    state.scroll_right();
                }
                while state.horizontal_offset() > target_column {
                    state.scroll_left();
                }
                true
            }
        };
        if changed {
            self.refresh();
        }
    }

    fn set_visible_lines(&self, lines: usize) {
        self.change(|state| state.set_visible_lines(lines));
    }

    fn sync_scrollbars(&self, state: &ViewerState) {
        self.syncing.set(true);

        let total_lines = state.estimated_total_lines().max(1) as f64;
        let visible_lines = state.visible_lines().max(1) as f64;
        self.vertical.set_lower(0.0);
        self.vertical.set_upper(total_lines);
        self.vertical.set_page_size(visible_lines.min(total_lines));
        self.vertical.set_step_increment(1.0);
        self.vertical.set_page_increment(visible_lines.max(1.0));
        self.vertical.set_value(state.first_visible_line() as f64);

        self.horizontal.set_lower(0.0);
        self.horizontal.set_upper(4096.0);
        self.horizontal.set_page_size(120.0);
        self.horizontal.set_step_increment(8.0);
        self.horizontal.set_page_increment(64.0);
        self.horizontal.set_value(state.horizontal_offset() as f64);

        self.syncing.set(false);
    }
}

fn estimate_visible_lines(view: &gtk::TextView) -> usize {
    let height = view.height() as usize;
    if height < 120 {
        return 40;
    }

    (height / 20).max(1)
}
