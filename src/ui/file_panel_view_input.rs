use std::{
    rc::Rc,
    time::{Duration, Instant},
};

use gtk::{gdk, EventControllerKey, glib, prelude::*};

use super::{
    helpers::{key_to_search_char, push_quick_search_text},
    PanelKeyNavigation, FilePanelView,
};

impl FilePanelView {
    pub fn connect_open_key<F, Q>(&self, on_nav: F, on_quick_search: Q)
    where
        F: Fn(PanelKeyNavigation) + 'static,
        Q: Fn(&str) -> bool + 'static,
    {
        let quick_search = Rc::clone(&self.quick_search);
        let on_nav = Rc::new(on_nav);
        let on_quick_search = Rc::new(on_quick_search);
        let last_space_action = Rc::clone(&self.last_space_action);
        let inline_rename_active = Rc::clone(&self.inline_rename_active);
        let emit_space = {
            let on_nav = Rc::clone(&on_nav);
            let last_space_action = Rc::clone(&last_space_action);
            move || {
                let now = Instant::now();
                if last_space_action
                    .get()
                    .is_some_and(|at| now.duration_since(at) < Duration::from_millis(80))
                {
                    return;
                }
                last_space_action.set(Some(now));
                on_nav(PanelKeyNavigation::SpaceAction);
            }
        };
        let emit_space = Rc::new(emit_space);
        let controller = EventControllerKey::new();
        controller.set_propagation_phase(gtk::PropagationPhase::Capture);

        let im_context = gtk::IMMulticontext::new();
        im_context.set_client_widget(Some(&self.column_view));
        controller.set_im_context(Some(&im_context));
        *self.panel_im_context.borrow_mut() = Some(im_context.clone());
        *self.panel_key_controller.borrow_mut() = Some(controller.clone());
        {
            // On Windows the IM often consumes Space as commit(" ") before key-pressed
            // sees it — treat whitespace-only commits as SpaceAction, not quick-search.
            let quick_search = Rc::clone(&quick_search);
            let emit_space = Rc::clone(&emit_space);
            let on_nav = Rc::clone(&on_nav);
            let on_quick_search = Rc::clone(&on_quick_search);
            let inline_rename_active = Rc::clone(&inline_rename_active);
            im_context.connect_commit(move |_, text| {
                if inline_rename_active.get() {
                    return;
                }
                if text.chars().all(|ch| ch.is_whitespace()) {
                    quick_search.borrow_mut().clear();
                    emit_space();
                    return;
                }
                if text == "*" {
                    quick_search.borrow_mut().clear();
                    on_nav(PanelKeyNavigation::InvertMarks);
                    return;
                }
                push_quick_search_text(&quick_search, on_quick_search.as_ref(), text);
            });
        }

        {
            let quick_search = Rc::clone(&quick_search);
            let on_nav = Rc::clone(&on_nav);
            let emit_space = Rc::clone(&emit_space);
            let on_quick_search = Rc::clone(&on_quick_search);
            let inline_rename_active = Rc::clone(&inline_rename_active);
            let column_view = self.column_view.clone();
            controller.connect_key_pressed(move |_, key, _, modifiers| {
                // Let the inline-rename Entry own Left/Right/typing; otherwise Capture
                // steals navigation keys and IM/quick-search can crash the UI.
                if inline_rename_active.get() {
                    return glib::Propagation::Proceed;
                }
                let action = match key {
                    gdk::Key::Return | gdk::Key::KP_Enter => {
                        // Alt+Enter → Properties; stop so ColumnView does not open the item.
                        if modifiers.contains(gdk::ModifierType::ALT_MASK)
                            && !modifiers.intersects(
                                gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SUPER_MASK,
                            )
                        {
                            let _ = WidgetExt::activate_action(
                                &column_view,
                                "win.properties",
                                None,
                            );
                            return glib::Propagation::Stop;
                        }
                        Some(PanelKeyNavigation::ActivateSelected)
                    }
                    gdk::Key::Right | gdk::Key::KP_Right | gdk::Key::rightarrow => {
                        Some(PanelKeyNavigation::EnterSelected)
                    }
                    gdk::Key::Left | gdk::Key::KP_Left | gdk::Key::leftarrow => {
                        Some(PanelKeyNavigation::GoParent)
                    }
                    gdk::Key::space | gdk::Key::KP_Space => {
                        quick_search.borrow_mut().clear();
                        emit_space();
                        return glib::Propagation::Stop;
                    }
                    gdk::Key::Insert | gdk::Key::KP_Insert => Some(PanelKeyNavigation::ToggleMark),
                    gdk::Key::asterisk | gdk::Key::KP_Multiply => {
                        Some(PanelKeyNavigation::InvertMarks)
                    }
                    gdk::Key::Menu | gdk::Key::MenuKB => Some(PanelKeyNavigation::ContextMenu),
                    gdk::Key::F10
                        if modifiers.contains(gdk::ModifierType::SHIFT_MASK)
                            && !modifiers.intersects(
                                gdk::ModifierType::CONTROL_MASK
                                    | gdk::ModifierType::ALT_MASK
                                    | gdk::ModifierType::SUPER_MASK,
                            ) =>
                    {
                        Some(PanelKeyNavigation::ContextMenu)
                    }
                    _ if key.to_unicode() == Some(' ') => {
                        quick_search.borrow_mut().clear();
                        emit_space();
                        return glib::Propagation::Stop;
                    }
                    _ if key.to_unicode() == Some('*')
                        && !modifiers.intersects(
                            gdk::ModifierType::CONTROL_MASK
                                | gdk::ModifierType::ALT_MASK
                                | gdk::ModifierType::SUPER_MASK,
                        ) =>
                    {
                        Some(PanelKeyNavigation::InvertMarks)
                    }
                    _ => None,
                };
                if let Some(action) = action {
                    quick_search.borrow_mut().clear();
                    on_nav(action);
                    return glib::Propagation::Stop;
                }

                if modifiers.intersects(
                    gdk::ModifierType::CONTROL_MASK
                        | gdk::ModifierType::ALT_MASK
                        | gdk::ModifierType::SUPER_MASK,
                ) {
                    return glib::Propagation::Proceed;
                }

                let mut buffer = quick_search.borrow_mut();
                buffer.expire_if_needed();

                if key == gdk::Key::Escape {
                    if buffer.query.is_empty() {
                        return glib::Propagation::Proceed;
                    }
                    buffer.clear();
                    drop(buffer);
                    let _ = on_quick_search("");
                    return glib::Propagation::Stop;
                }

                if key == gdk::Key::BackSpace {
                    if buffer.query.is_empty() {
                        return glib::Propagation::Proceed;
                    }
                    buffer.query.pop();
                    let query = buffer.query.clone();
                    if query.is_empty() {
                        buffer.clear();
                    } else {
                        buffer.last_input = Some(Instant::now());
                    }
                    drop(buffer);
                    let _ = on_quick_search(&query);
                    return glib::Propagation::Stop;
                }

                // Fallback when the IM context does not consume the key (common on Windows).
                if let Some(ch) = key_to_search_char(key) {
                    let mut next = buffer.query.clone();
                    next.push(ch);
                    drop(buffer);
                    if on_quick_search(&next) {
                        quick_search.borrow_mut().set_query(next);
                    }
                    return glib::Propagation::Stop;
                }

                glib::Propagation::Proceed
            });
        }

        self.column_view.add_controller(controller);

        {
            let im_context_for_focus = im_context.clone();
            self.column_view.connect_has_focus_notify(move |view| {
                if view.has_focus() {
                    im_context_for_focus.focus_in();
                } else {
                    im_context_for_focus.focus_out();
                }
            });
            if self.column_view.has_focus() {
                im_context.focus_in();
            }
        }
    }
}
