use std::{
    cell::{Cell, RefCell},
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

use anyhow::Result;
use gtk::{gdk, prelude::*};
use rust_i18n::t;

use crate::{
    archive::ArchiveService,
    config::{ThemePreference, ViewerConfig},
    ui::{
        dialogs::{independent_window, present_and_focus},
        dsl::{frame, hbox, scrollbar, text, text_view, Property},
        file_viewer_archive, file_viewer_code,
        file_viewer_common::{
            append_status, build_viewer_chrome, file_size, install_escape_close,
            read_text_file_limited,
        },
        file_viewer_csv,
        file_viewer_html::{html_to_preview_markdown, is_html_path},
        file_viewer_image,
        file_viewer_markdown::{is_markdown_path, render_markdown_into_buffer},
        file_viewer_structured::{self, StructuredKind},
    },
    viewer::ViewerState,
};

enum FormattedKind {
    Markdown,
    Html,
}

/// Opens `path` in the viewer that fits its format; whatever has no special
/// viewer goes to the streaming one.
pub fn open(
    parent: &gtk::ApplicationWindow,
    path: PathBuf,
    config: ViewerConfig,
    theme: ThemePreference,
    archive_service: &ArchiveService,
    sibling_images: Vec<PathBuf>,
) -> Result<()> {
    let max_bytes = config.streaming_threshold_bytes();

    if file_viewer_image::is_image_path(&path) {
        return file_viewer_image::open(parent, &path, sibling_images);
    }
    if archive_service.can_open_path(&path) {
        return file_viewer_archive::open(parent, &path, archive_service);
    }
    if is_markdown_path(&path) {
        return open_formatted(parent, path, FormattedKind::Markdown, max_bytes);
    }
    if is_html_path(&path) {
        return open_formatted(parent, path, FormattedKind::Html, max_bytes);
    }
    if file_viewer_csv::is_csv_path(&path) {
        return file_viewer_csv::open(parent, &path, max_bytes, theme);
    }
    if let Some(kind) = file_viewer_structured::detect_structured_kind(&path) {
        return open_structured(parent, &path, kind, max_bytes, theme);
    }
    if file_viewer_code::can_open_as_code(&path, file_size(&path), max_bytes) {
        return file_viewer_code::open(
            parent,
            &path,
            max_bytes,
            theme,
            &t!("viewer.mode_code").into_owned(),
        );
    }

    open_streaming(parent, path, config)
}

fn open_structured(
    parent: &gtk::ApplicationWindow,
    path: &Path,
    kind: StructuredKind,
    max_bytes: u64,
    theme: ThemePreference,
) -> Result<()> {
    match file_viewer_structured::open(parent, path, kind, max_bytes, theme) {
        Ok(()) => Ok(()),
        // Raw highlighted text is still useful when pretty-printing fails.
        Err(_) if file_size(path) <= max_bytes => file_viewer_code::open(
            parent,
            path,
            max_bytes,
            theme,
            &t!("viewer.mode_code").into_owned(),
        ),
        Err(error) => Err(error),
    }
}

fn open_formatted(
    parent: &gtk::ApplicationWindow,
    path: PathBuf,
    kind: FormattedKind,
    max_bytes: u64,
) -> Result<()> {
    let source = read_text_file_limited(&path, max_bytes)?;
    let (preview_markdown, mode) = match kind {
        FormattedKind::Markdown => (source, t!("viewer.mode_markdown").into_owned()),
        FormattedKind::Html => (
            html_to_preview_markdown(&source)?,
            t!("viewer.mode_html").into_owned(),
        ),
    };

    let chrome = build_viewer_chrome(parent, &path);
    let buffer = gtk::TextBuffer::new(None);
    render_markdown_into_buffer(&buffer, &preview_markdown);

    let view = gtk::TextView::with_buffer(&buffer);
    view.set_editable(false);
    view.set_cursor_visible(false);
    view.set_wrap_mode(gtk::WrapMode::WordChar);
    view.set_hexpand(true);
    view.set_vexpand(true);
    view.add_css_class("editor-view");
    view.set_left_margin(12);
    view.set_right_margin(12);
    view.set_top_margin(10);
    view.set_bottom_margin(10);

    let scrolled = gtk::ScrolledWindow::builder()
        .child(&view)
        .hexpand(true)
        .vexpand(true)
        .build();
    scrolled.add_css_class("panel-scroller");
    chrome.content.append(&scrolled);

    append_status(&chrome.content, &mode, file_size(&path));
    install_escape_close(&chrome.window);
    present_and_focus(chrome.window.upcast_ref(), &view);
    Ok(())
}

/// The streaming viewer renders a window of lines itself instead of loading
/// the file into the text view, so the scrollbars drive [`ViewerState`] and
/// the text view only shows what it rendered.
fn open_streaming(
    parent: &gtk::ApplicationWindow,
    path: PathBuf,
    config: ViewerConfig,
) -> Result<()> {
    let viewer = Rc::new(Viewer::open(&path, &config)?);

    let (window, view) =
        independent_window(parent, &t!("viewer.window_title"), (980, 720), |dialog| {
            let window = dialog.window();
            {
                let title_label = dialog.title_label().clone();
                window.bind(&viewer.title, move |window, title| {
                    if !title.is_empty() {
                        window.set_title(Some(title.as_str()));
                        title_label.set_label(title);
                        title_label.set_tooltip_text(Some(title));
                    }
                });
            }
            {
                let viewer = Rc::clone(&viewer);
                let close: gtk::Window = window.widget().clone().upcast();
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
                            view.on_scroll(
                                gtk::EventControllerScrollFlags::VERTICAL,
                                move |_, dy| scroller.scroll_vertically(dy),
                            );
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
    present_and_focus(window.upcast_ref(), view.widget());
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
    fn open(path: &Path, config: &ViewerConfig) -> Result<Self> {
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
