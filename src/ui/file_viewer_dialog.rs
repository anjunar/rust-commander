use std::{cell::RefCell, path::PathBuf, rc::Rc, time::Duration};

use anyhow::Result;
use gtk::{gdk, glib, prelude::*};

use crate::{ui::dialogs::build_modal_window, viewer::ViewerState};

pub fn open(parent: &gtk::ApplicationWindow, path: PathBuf) -> Result<()> {
    let state = Rc::new(RefCell::new(ViewerState::open(&path)?));
    let syncing_adjustments = Rc::new(std::cell::Cell::new(false));
    let vertical_adjustment = gtk::Adjustment::new(0.0, 0.0, 1.0, 1.0, 1.0, 1.0);
    let horizontal_adjustment = gtk::Adjustment::new(0.0, 0.0, 4096.0, 8.0, 64.0, 120.0);

    let modal = build_modal_window(parent, "Viewer", 980, 720);
    let dialog = modal.window;
    let content = modal.content;
    let actions = modal.actions;

    let path_label = gtk::Label::new(Some(&path.display().to_string()));
    path_label.set_xalign(0.0);
    path_label.add_css_class("path-label");
    content.append(&path_label);

    let buffer = gtk::TextBuffer::new(None);
    let view = gtk::TextView::with_buffer(&buffer);
    view.set_editable(false);
    view.set_cursor_visible(false);
    view.set_monospace(true);
    view.set_wrap_mode(gtk::WrapMode::None);
    view.set_hexpand(true);
    view.set_vexpand(true);
    view.add_css_class("editor-view");
    view.set_left_margin(8);
    view.set_top_margin(8);
    view.set_bottom_margin(8);

    let status_label = gtk::Label::new(None);
    status_label.set_xalign(0.0);
    status_label.add_css_class("editor-status");

    let horizontal_scroll = gtk::EventControllerScroll::new(
        gtk::EventControllerScrollFlags::HORIZONTAL | gtk::EventControllerScrollFlags::DISCRETE,
    );
    {
        let dialog = dialog.clone();
        let state = Rc::clone(&state);
        let buffer = buffer.clone();
        let status_label = status_label.clone();
        let vertical_adjustment = vertical_adjustment.clone();
        let horizontal_adjustment = horizontal_adjustment.clone();
        let syncing_adjustments = Rc::clone(&syncing_adjustments);
        horizontal_scroll.connect_scroll(move |_, dx, _| {
            {
                let mut state = state.borrow_mut();
                if dx > 0.0 {
                    state.scroll_right();
                } else if dx < 0.0 {
                    state.scroll_left();
                } else {
                    return glib::Propagation::Proceed;
                }
            }

            render_into_widgets(
                &dialog,
                &state,
                &buffer,
                &status_label,
                &vertical_adjustment,
                &horizontal_adjustment,
                &syncing_adjustments,
            );
            glib::Propagation::Stop
        });
    }
    view.add_controller(horizontal_scroll);

    let view_frame = gtk::Frame::new(None);
    view_frame.set_hexpand(true);
    view_frame.set_vexpand(true);
    view_frame.add_css_class("panel-scroller");
    view_frame.set_child(Some(&view));

    let vertical_scrollbar = gtk::Scrollbar::builder()
        .orientation(gtk::Orientation::Vertical)
        .adjustment(&vertical_adjustment)
        .build();

    let viewer_area = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    viewer_area.set_hexpand(true);
    viewer_area.set_vexpand(true);
    viewer_area.append(&view_frame);
    viewer_area.append(&vertical_scrollbar);
    content.append(&viewer_area);

    let horizontal_scrollbar = gtk::Scrollbar::builder()
        .orientation(gtk::Orientation::Horizontal)
        .adjustment(&horizontal_adjustment)
        .build();
    content.append(&horizontal_scrollbar);

    status_label.set_hexpand(true);
    content.append(&status_label);

    let close_button = gtk::Button::with_label("Close");
    close_button.add_css_class("command-button");
    {
        let dialog = dialog.clone();
        close_button.connect_clicked(move |_| {
            dialog.close();
        });
    }
    actions.append(&close_button);
    dialog.set_default_widget(Some(&close_button));

    render_into_widgets(
        &dialog,
        &state,
        &buffer,
        &status_label,
        &vertical_adjustment,
        &horizontal_adjustment,
        &syncing_adjustments,
    );

    let key_controller = gtk::EventControllerKey::new();
    {
        let dialog = dialog.clone();
        let state = Rc::clone(&state);
        let buffer = buffer.clone();
        let status_label = status_label.clone();
        let vertical_adjustment = vertical_adjustment.clone();
        let horizontal_adjustment = horizontal_adjustment.clone();
        let syncing_adjustments = Rc::clone(&syncing_adjustments);
        key_controller.connect_key_pressed(move |_, key, _, _| {
            let handled = {
                let mut state = state.borrow_mut();
                match key {
                    gdk::Key::Escape => {
                        dialog.close();
                        return glib::Propagation::Stop;
                    }
                    gdk::Key::Up => state.scroll_line_up(),
                    gdk::Key::Down => state.scroll_line_down(),
                    gdk::Key::Page_Up => state.page_up(),
                    gdk::Key::Page_Down => state.page_down(),
                    gdk::Key::Home => state.go_to_start(),
                    gdk::Key::End => {
                        if let Err(error) = state.go_to_end() {
                            status_label.set_label(&format!("Viewer error: {error}"));
                            return glib::Propagation::Stop;
                        }
                    }
                    gdk::Key::Left => state.scroll_left(),
                    gdk::Key::Right => state.scroll_right(),
                    gdk::Key::F2 => state.toggle_hex_mode(),
                    _ => return glib::Propagation::Proceed,
                }
                true
            };

            if handled {
                render_into_widgets(
                    &dialog,
                    &state,
                    &buffer,
                    &status_label,
                    &vertical_adjustment,
                    &horizontal_adjustment,
                    &syncing_adjustments,
                );
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
    }
    dialog.add_controller(key_controller);

    let scroll_controller =
        gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    {
        let dialog = dialog.clone();
        let state = Rc::clone(&state);
        let buffer = buffer.clone();
        let status_label = status_label.clone();
        let vertical_adjustment = vertical_adjustment.clone();
        let horizontal_adjustment = horizontal_adjustment.clone();
        let syncing_adjustments = Rc::clone(&syncing_adjustments);
        scroll_controller.connect_scroll(move |_, _, dy| {
            {
                let mut state = state.borrow_mut();
                if dy > 0.0 {
                    state.scroll_line_down();
                } else if dy < 0.0 {
                    state.scroll_line_up();
                } else {
                    return glib::Propagation::Proceed;
                }
            }

            render_into_widgets(
                &dialog,
                &state,
                &buffer,
                &status_label,
                &vertical_adjustment,
                &horizontal_adjustment,
                &syncing_adjustments,
            );
            glib::Propagation::Stop
        });
    }
    view.add_controller(scroll_controller);

    {
        let dialog = dialog.clone();
        let state = Rc::clone(&state);
        let buffer = buffer.clone();
        let status_label = status_label.clone();
        let vertical_adjustment = vertical_adjustment.clone();
        let horizontal_adjustment = horizontal_adjustment.clone();
        let syncing_adjustments = Rc::clone(&syncing_adjustments);
        let adjustment_for_signal = vertical_adjustment.clone();
        adjustment_for_signal.connect_value_changed(move |adjustment| {
            if syncing_adjustments.get() {
                return;
            }

            let target_line = adjustment.value().round().max(0.0) as usize;
            let changed = {
                let mut state = state.borrow_mut();
                if target_line == state.first_visible_line() {
                    false
                } else if state.set_first_visible_line(target_line).is_ok() {
                    true
                } else {
                    status_label.set_label("Viewer error: Could not update scroll position");
                    false
                }
            };

            if changed {
                render_into_widgets(
                    &dialog,
                    &state,
                    &buffer,
                    &status_label,
                    &vertical_adjustment,
                    &horizontal_adjustment,
                    &syncing_adjustments,
                );
            }
        });
    }

    {
        let dialog = dialog.clone();
        let state = Rc::clone(&state);
        let buffer = buffer.clone();
        let status_label = status_label.clone();
        let vertical_adjustment = vertical_adjustment.clone();
        let horizontal_adjustment = horizontal_adjustment.clone();
        let syncing_adjustments = Rc::clone(&syncing_adjustments);
        let adjustment_for_signal = horizontal_adjustment.clone();
        adjustment_for_signal.connect_value_changed(move |adjustment| {
            if syncing_adjustments.get() {
                return;
            }

            let changed = {
                let mut state = state.borrow_mut();
                let target_column = adjustment.value().round().max(0.0) as usize;
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
                render_into_widgets(
                    &dialog,
                    &state,
                    &buffer,
                    &status_label,
                    &vertical_adjustment,
                    &horizontal_adjustment,
                    &syncing_adjustments,
                );
            }
        });
    }

    {
        let dialog = dialog.clone();
        let state = Rc::clone(&state);
        let buffer = buffer.clone();
        let status_label = status_label.clone();
        let view = view.clone();
        let vertical_adjustment = vertical_adjustment.clone();
        let horizontal_adjustment = horizontal_adjustment.clone();
        let syncing_adjustments = Rc::clone(&syncing_adjustments);
        glib::timeout_add_local_once(Duration::from_millis(60), move || {
            let visible_lines = estimate_visible_lines(&view);
            state.borrow_mut().set_visible_lines(visible_lines);
            render_into_widgets(
                &dialog,
                &state,
                &buffer,
                &status_label,
                &vertical_adjustment,
                &horizontal_adjustment,
                &syncing_adjustments,
            );
        });
    }

    glib::idle_add_local_once(move || {
        dialog.present();
        view.grab_focus();
    });

    Ok(())
}

fn render_into_widgets(
    dialog: &gtk::Window,
    state: &Rc<RefCell<ViewerState>>,
    buffer: &gtk::TextBuffer,
    status_label: &gtk::Label,
    vertical_adjustment: &gtk::Adjustment,
    horizontal_adjustment: &gtk::Adjustment,
    syncing_adjustments: &Rc<std::cell::Cell<bool>>,
) {
    let mut state = state.borrow_mut();
    match state.render() {
        Ok(rendered) => {
            dialog.set_title(Some(&rendered.title));
            buffer.set_text(&rendered.body);
            status_label.set_label(&rendered.status);
            sync_scrollbars(
                vertical_adjustment,
                horizontal_adjustment,
                &state,
                syncing_adjustments,
            );
        }
        Err(error) => {
            buffer.set_text(&format!("Could not render file view.\n\n{error}"));
            status_label.set_label(&format!("Viewer error: {error}"));
        }
    }
}

fn estimate_visible_lines(view: &gtk::TextView) -> usize {
    let height = view.height() as usize;
    if height < 120 {
        return 40;
    }

    (height / 20).max(1)
}

fn sync_scrollbars(
    vertical: &gtk::Adjustment,
    horizontal: &gtk::Adjustment,
    state: &ViewerState,
    syncing_adjustments: &Rc<std::cell::Cell<bool>>,
) {
    syncing_adjustments.set(true);

    let total_lines = state.estimated_total_lines().max(1) as f64;
    let visible_lines = state.visible_lines().max(1) as f64;
    vertical.set_lower(0.0);
    vertical.set_upper(total_lines);
    vertical.set_page_size(visible_lines.min(total_lines));
    vertical.set_step_increment(1.0);
    vertical.set_page_increment(visible_lines.max(1.0));
    vertical.set_value(state.first_visible_line() as f64);

    horizontal.set_lower(0.0);
    horizontal.set_upper(4096.0);
    horizontal.set_page_size(120.0);
    horizontal.set_step_increment(8.0);
    horizontal.set_page_increment(64.0);
    horizontal.set_value(state.horizontal_offset() as f64);

    syncing_adjustments.set(false);
}
