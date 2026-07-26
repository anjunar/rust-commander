use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

use gtk::{gdk, EventControllerKey, glib, prelude::*};

use crate::ui::{
    columns::{begin_inline_rename, ROW_POSITION_DATA_KEY},
    file_row_object::FileRowObject,
};

use super::QuickSearchBuffer;

pub(super) fn find_row_position(widget: &gtk::Widget, root: &gtk::ColumnView) -> Option<usize> {
    let mut current = Some(widget.clone());
    while let Some(candidate) = current {
        unsafe {
            if let Some(position) = candidate.data::<usize>(ROW_POSITION_DATA_KEY) {
                return Some(*position.as_ref());
            }
        }

        if candidate == *root.upcast_ref::<gtk::Widget>() {
            break;
        }
        current = candidate.parent();
    }
    None
}

pub(super) fn push_quick_search_text<Q>(
    buffer: &RefCell<QuickSearchBuffer>,
    on_quick_search: &Q,
    text: &str,
) where
    Q: Fn(&str) -> bool + ?Sized,
{
    let text = text.trim();
    if text.is_empty() {
        return;
    }

    let mut state = buffer.borrow_mut();
    state.expire_if_needed();
    let mut next = state.query.clone();
    next.push_str(text);
    drop(state);

    if on_quick_search(&next) {
        buffer.borrow_mut().set_query(next);
    }
}

pub(super) fn key_to_search_char(key: gdk::Key) -> Option<char> {
    key.to_unicode()
        .filter(|ch| !ch.is_control() && !ch.is_whitespace() && *ch != '*')
}

pub(super) fn restore_panel_key_input(
    column_view: &gtk::ColumnView,
    panel_im_context: &RefCell<Option<gtk::IMMulticontext>>,
    panel_key_controller: &RefCell<Option<EventControllerKey>>,
) {
    if let Some(im) = panel_im_context.borrow().as_ref() {
        im.set_client_widget(Some(column_view));
        if let Some(controller) = panel_key_controller.borrow().as_ref() {
            controller.set_im_context(Some(&im.clone().upcast::<gtk::IMContext>()));
        }
    }
}

pub(super) fn start_inline_rename_with_retry<F>(
    row: FileRowObject,
    column_view: gtk::ColumnView,
    name: String,
    is_dir: bool,
    is_marked: bool,
    on_commit: Rc<RefCell<Option<F>>>,
    cancel_slot: Rc<RefCell<Option<Rc<dyn Fn()>>>>,
    active: Rc<Cell<bool>>,
    panel_im_context: Rc<RefCell<Option<gtk::IMMulticontext>>>,
    panel_key_controller: Rc<RefCell<Option<EventControllerKey>>>,
    attempts_left: u32,
) where
    F: FnOnce(String) + 'static,
{
    if let Some(container) = row.name_cell() {
        let Some(on_commit) = on_commit.borrow_mut().take() else {
            return;
        };
        let cancel = begin_inline_rename(
            &container,
            &name,
            is_dir,
            is_marked,
            on_commit,
            {
                let active = Rc::clone(&active);
                let cancel_slot = Rc::clone(&cancel_slot);
                let im = Rc::clone(&panel_im_context);
                let keys = Rc::clone(&panel_key_controller);
                let column_view = column_view.clone();
                move || {
                    active.set(false);
                    let _ = cancel_slot.borrow_mut().take();
                    // Move focus off the dying cell before list refresh/rebind.
                    column_view.grab_focus();
                    restore_panel_key_input(&column_view, &im, &keys);
                }
            },
        );
        *cancel_slot.borrow_mut() = Some(cancel);
        return;
    }

    if attempts_left == 0 {
        active.set(false);
        restore_panel_key_input(&column_view, &panel_im_context, &panel_key_controller);
        return;
    }

    glib::timeout_add_local_once(Duration::from_millis(16), move || {
        start_inline_rename_with_retry(
            row,
            column_view,
            name,
            is_dir,
            is_marked,
            on_commit,
            cancel_slot,
            active,
            panel_im_context,
            panel_key_controller,
            attempts_left - 1,
        );
    });
}

pub(super) fn install_root_selector_escape(dropdown: &gtk::DropDown, column_view: &gtk::ColumnView) {
    let restore_panel_focus = {
        let column_view = column_view.clone();
        Rc::new(move || {
            let column_view = column_view.clone();
            glib::idle_add_local_once(move || {
                column_view.grab_focus();
            });
        })
    };

    let attach_escape = |widget: &gtk::Widget| {
        let controller = EventControllerKey::new();
        controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        let dropdown = dropdown.clone();
        controller.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape && close_dropdown_popup(&dropdown) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        widget.add_controller(controller);
    };

    attach_escape(dropdown.upcast_ref());

    let mut child = dropdown.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if let Ok(popover) = widget.downcast::<gtk::Popover>() {
            attach_escape(popover.upcast_ref());
            let restore_panel_focus = Rc::clone(&restore_panel_focus);
            popover.connect_closed(move |_| {
                restore_panel_focus();
            });
        }
    }
}

fn close_dropdown_popup(dropdown: &gtk::DropDown) -> bool {
    let mut closed = false;
    let mut child = dropdown.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if let Ok(button) = widget.clone().downcast::<gtk::ToggleButton>() {
            if button.is_active() {
                button.set_active(false);
                closed = true;
            }
        } else if let Ok(popover) = widget.downcast::<gtk::Popover>() {
            if popover.is_visible() {
                popover.popdown();
                closed = true;
            }
        }
    }
    closed
}

/// Defer scroll_to(FOCUS) until ColumnView has a real height. Early calls during
/// startup/paned layout leave unbound rows as a solid theme-colored rectangle.
pub(super) fn schedule_column_view_focus(column_view: gtk::ColumnView, index: u32) {
    schedule_column_view_focus_attempt(column_view, index, 0);
}

fn schedule_column_view_focus_attempt(
    column_view: gtk::ColumnView,
    index: u32,
    attempt: u32,
) {
    const MAX_ATTEMPTS: u32 = 30;
    let run = {
        let column_view = column_view.clone();
        move || {
            if column_view.height() < 48 && attempt < MAX_ATTEMPTS {
                schedule_column_view_focus_attempt(column_view, index, attempt + 1);
                return;
            }
            column_view.scroll_to(index, None, gtk::ListScrollFlags::FOCUS, None);
            column_view.queue_draw();
        }
    };
    if attempt == 0 {
        glib::idle_add_local_once(run);
    } else {
        glib::timeout_add_local_once(Duration::from_millis(16), run);
    }
}
