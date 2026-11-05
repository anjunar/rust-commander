use std::{rc::Rc, time::Duration};

use gtk::{gdk, glib, prelude::*, EventControllerKey};

/// Escape closes the picker's popup, and focus returns to the file list.
pub(super) fn install_root_selector_escape(dropdown: &gtk::DropDown, view: &gtk::ColumnView) {
    let restore_panel_focus = {
        let view = view.clone();
        Rc::new(move || {
            let view = view.clone();
            glib::idle_add_local_once(move || {
                view.grab_focus();
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

/// Waits with `scroll_to(FOCUS)` until the view has a real height. Earlier
/// calls, during startup or a paned layout, leave unbound rows as a solid
/// theme-coloured rectangle.
pub(super) fn schedule_column_view_focus(view: gtk::ColumnView, index: u32) {
    schedule_focus_attempt(view, index, 0);
}

fn schedule_focus_attempt(view: gtk::ColumnView, index: u32, attempt: u32) {
    const MAX_ATTEMPTS: u32 = 30;
    let run = {
        let view = view.clone();
        move || {
            if view.height() < 48 && attempt < MAX_ATTEMPTS {
                schedule_focus_attempt(view, index, attempt + 1);
                return;
            }
            view.scroll_to(index, None, gtk::ListScrollFlags::FOCUS, None);
            view.queue_draw();
        }
    };
    if attempt == 0 {
        glib::idle_add_local_once(run);
    } else {
        glib::timeout_add_local_once(Duration::from_millis(16), run);
    }
}
