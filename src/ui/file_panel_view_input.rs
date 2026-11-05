//! The keyboard of a panel: navigation keys, marks, and quick search.
//!
//! Printable keys reach the panel twice on some platforms, once through the
//! input method and once as key presses, so both paths lead to the same
//! handling here.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};

use gtk::{gdk, glib, prelude::*, EventControllerKey};

use crate::{
    application::ActivePanel,
    ui::{dsl::Table, shell::ShellEvents},
};

use super::PanelKeyNavigation;

const QUICK_SEARCH_TIMEOUT: Duration = Duration::from_millis(1200);

/// Space can arrive as an input method commit and as a key press.
const SPACE_DEBOUNCE: Duration = Duration::from_millis(80);

#[derive(Default)]
struct QuickSearchBuffer {
    query: String,
    last_input: Option<Instant>,
}

impl QuickSearchBuffer {
    fn expire_if_needed(&mut self) {
        if self
            .last_input
            .is_some_and(|at| at.elapsed() >= QUICK_SEARCH_TIMEOUT)
        {
            self.clear();
        }
    }

    fn clear(&mut self) {
        self.query.clear();
        self.last_input = None;
    }

    fn set_query(&mut self, query: String) {
        self.query = query;
        self.last_input = Some(Instant::now());
    }
}

/// Input state of one panel, shared by its key handlers and its inline rename.
pub(super) struct PanelInput {
    quick_search: RefCell<QuickSearchBuffer>,
    last_space_action: Cell<Option<Instant>>,
    renaming: Cell<bool>,
    rename_cancel: RefCell<Option<Rc<dyn Fn()>>>,
    im_context: gtk::IMMulticontext,
    keys: EventControllerKey,
}

impl PanelInput {
    pub(super) fn new() -> Self {
        let keys = EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        Self {
            quick_search: RefCell::default(),
            last_space_action: Cell::new(None),
            renaming: Cell::new(false),
            rename_cancel: RefCell::new(None),
            im_context: gtk::IMMulticontext::new(),
            keys,
        }
    }

    pub(super) fn clear_quick_search(&self) {
        self.quick_search.borrow_mut().clear();
    }

    /// Takes the input method off the key controller, which would otherwise
    /// filter printable keys away from the rename entry in the capture phase.
    ///
    /// The context keeps its client widget: dropping it leaves the Windows IME
    /// filter of a freed context registered, and GTK then calls that filter
    /// for every window message.
    pub(super) fn suspend(&self) {
        self.renaming.set(true);
        self.keys.set_im_context(Option::<&gtk::IMContext>::None);
        self.im_context.reset();
    }

    pub(super) fn resume(&self) {
        self.renaming.set(false);
        self.keys.set_im_context(Some(&self.im_context));
    }

    pub(super) fn set_rename_cancel(&self, cancel: Rc<dyn Fn()>) {
        *self.rename_cancel.borrow_mut() = Some(cancel);
    }

    pub(super) fn take_rename_cancel(&self) -> Option<Rc<dyn Fn()>> {
        self.rename_cancel.borrow_mut().take()
    }

    fn space_is_repeated(&self) -> bool {
        let now = Instant::now();
        let repeated = self
            .last_space_action
            .get()
            .is_some_and(|at| now.duration_since(at) < SPACE_DEBOUNCE);
        if !repeated {
            self.last_space_action.set(Some(now));
        }
        repeated
    }
}

/// Connects the panel's key controller and input method to `table`.
pub(super) fn install_panel_keys(
    table: &Table,
    panel: ActivePanel,
    input: &Rc<PanelInput>,
    events: &Rc<dyn ShellEvents>,
) {
    let emit_space = {
        let input = Rc::clone(input);
        let events = Rc::clone(events);
        Rc::new(move || {
            input.clear_quick_search();
            if !input.space_is_repeated() {
                events.panel_key(panel, PanelKeyNavigation::SpaceAction);
            }
        })
    };

    input.im_context.set_client_widget(Some(table.widget()));
    input.keys.set_im_context(Some(&input.im_context));

    // On Windows the input method often consumes Space as a commit before
    // key-pressed sees it, so a blank commit means Space, not a search.
    {
        let handler_input = Rc::clone(input);
        let events = Rc::clone(events);
        let emit_space = Rc::clone(&emit_space);
        let handler = input.im_context.connect_commit(move |_, committed| {
            let input = &handler_input;
            if input.renaming.get() {
                return;
            }
            if committed.chars().all(char::is_whitespace) {
                emit_space();
            } else if committed == "*" {
                input.clear_quick_search();
                events.panel_key(panel, PanelKeyNavigation::InvertMarks);
            } else {
                push_quick_search_text(input, panel, &events, committed);
            }
        });
        table.own_object_signal(&input.im_context, handler);
    }

    {
        let handler_input = Rc::clone(input);
        let events = Rc::clone(events);
        let view = table.widget().clone();
        let handler = input.keys.connect_key_pressed(move |_, key, _, modifiers| {
            let input = &handler_input;
            // The rename entry owns the arrows and typing while it is open.
            if input.renaming.get() {
                return glib::Propagation::Proceed;
            }
            handle_key(input, panel, &events, &emit_space, &view, key, modifiers)
        });
        table.own_object_signal(&input.keys, handler);
    }

    table.own_controller(input.keys.clone());

    {
        let im_context = input.im_context.clone();
        table.on_focus_change(move |focused| {
            if focused {
                im_context.focus_in();
            } else {
                im_context.focus_out();
            }
        });
    }
    if table.has_focus() {
        input.im_context.focus_in();
    }
}

fn handle_key(
    input: &PanelInput,
    panel: ActivePanel,
    events: &Rc<dyn ShellEvents>,
    emit_space: &Rc<impl Fn() + ?Sized>,
    view: &gtk::ColumnView,
    key: gdk::Key,
    modifiers: gdk::ModifierType,
) -> glib::Propagation {
    let commands = gdk::ModifierType::CONTROL_MASK
        | gdk::ModifierType::ALT_MASK
        | gdk::ModifierType::SUPER_MASK;

    let action = match key {
        gdk::Key::Return | gdk::Key::KP_Enter => {
            // Alt+Enter must not also open the item.
            if modifiers.contains(gdk::ModifierType::ALT_MASK)
                && !modifiers
                    .intersects(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SUPER_MASK)
            {
                let _ = WidgetExt::activate_action(view, "win.properties", None);
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
            emit_space();
            return glib::Propagation::Stop;
        }
        gdk::Key::Insert | gdk::Key::KP_Insert => Some(PanelKeyNavigation::ToggleMark),
        gdk::Key::asterisk | gdk::Key::KP_Multiply => Some(PanelKeyNavigation::InvertMarks),
        gdk::Key::Menu | gdk::Key::MenuKB => Some(PanelKeyNavigation::ContextMenu),
        gdk::Key::F10
            if modifiers.contains(gdk::ModifierType::SHIFT_MASK)
                && !modifiers.intersects(commands) =>
        {
            Some(PanelKeyNavigation::ContextMenu)
        }
        _ if key.to_unicode() == Some(' ') => {
            emit_space();
            return glib::Propagation::Stop;
        }
        _ if key.to_unicode() == Some('*') && !modifiers.intersects(commands) => {
            Some(PanelKeyNavigation::InvertMarks)
        }
        _ => None,
    };
    if let Some(action) = action {
        input.clear_quick_search();
        events.panel_key(panel, action);
        return glib::Propagation::Stop;
    }

    if modifiers.intersects(commands) {
        return glib::Propagation::Proceed;
    }

    let mut buffer = input.quick_search.borrow_mut();
    buffer.expire_if_needed();

    if key == gdk::Key::Escape {
        if buffer.query.is_empty() {
            return glib::Propagation::Proceed;
        }
        buffer.clear();
        drop(buffer);
        let _ = events.quick_search(panel, "");
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
        let _ = events.quick_search(panel, &query);
        return glib::Propagation::Stop;
    }

    // For keys the input method did not consume, common on Windows.
    if let Some(typed) = key_to_search_char(key) {
        let mut next = buffer.query.clone();
        next.push(typed);
        drop(buffer);
        if events.quick_search(panel, &next) {
            input.quick_search.borrow_mut().set_query(next);
        }
        return glib::Propagation::Stop;
    }

    glib::Propagation::Proceed
}

fn push_quick_search_text(
    input: &PanelInput,
    panel: ActivePanel,
    events: &Rc<dyn ShellEvents>,
    committed: &str,
) {
    let committed = committed.trim();
    if committed.is_empty() {
        return;
    }

    let next = {
        let mut buffer = input.quick_search.borrow_mut();
        buffer.expire_if_needed();
        format!("{}{committed}", buffer.query)
    };
    if events.quick_search(panel, &next) {
        input.quick_search.borrow_mut().set_query(next);
    }
}

fn key_to_search_char(key: gdk::Key) -> Option<char> {
    key.to_unicode()
        .filter(|typed| !typed.is_control() && !typed.is_whitespace() && *typed != '*')
}
