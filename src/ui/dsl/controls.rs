//! The remaining controls the commander's dialogs and windows are built from,
//! plus the event controllers and slot containers GTK needs for them.
//!
//! Slot containers (`Paned`, `Overlay`, `HeaderBar`, popovers) have more than
//! one place to put children. Their handles expose one method per slot, and
//! each method composes its body into that slot.

use std::rc::Rc;

use gtk::{gdk, glib, prelude::*};

use super::{
    components::{container, mount, Ctx, SyncGuard},
    scope::{self, Host},
    state::{Disposable, IntoReadOnly, IntoText, Property},
};

// ---------------------------------------------------------------------------
// Form controls
// ---------------------------------------------------------------------------

/// A password entry bound both ways to `value`.
pub fn password(
    value: &Property<String>,
    body: impl FnOnce(&Ctx<gtk::PasswordEntry>),
) -> Ctx<gtk::PasswordEntry> {
    mount(gtk::PasswordEntry::new(), |ctx| {
        ctx.bind_text(value);
        body(ctx);
    })
}

impl Ctx<gtk::PasswordEntry> {
    /// Two-way binding between the entry text and a string property.
    pub fn bind_text(&self, value: &Property<String>) {
        let guard = SyncGuard::default();
        {
            let guard = guard.clone();
            self.bind(value, move |entry, text| {
                // See `Ctx<gtk::Entry>::bind_text`: no echo of what was typed.
                if entry.text().as_str() != text.as_str() {
                    guard.run(|| entry.set_text(text));
                }
            });
        }
        let value = value.clone();
        let handler = self.widget().connect_changed(move |entry| {
            if !guard.is_syncing() {
                value.set(entry.text().to_string());
            }
        });
        self.own_signal(handler);
    }

    /// Runs when Enter is pressed in the entry.
    pub fn on_activate(&self, handler: impl Fn() + 'static) {
        let handler = self.widget().connect_activate(move |_| handler());
        self.own_signal(handler);
    }
}

/// A check button bound both ways to `value`.
pub fn check(
    label: impl IntoText,
    value: &Property<bool>,
    body: impl FnOnce(&Ctx<gtk::CheckButton>),
) -> Ctx<gtk::CheckButton> {
    mount(gtk::CheckButton::new(), |ctx| {
        ctx.bind(label.into_text(), |check, text| {
            check.set_label(Some(text.as_str()));
        });
        ctx.bind_active(value);
        body(ctx);
    })
}

impl Ctx<gtk::CheckButton> {
    /// Two-way binding between the check state and a boolean property.
    pub fn bind_active(&self, value: &Property<bool>) {
        let guard = SyncGuard::default();
        {
            let guard = guard.clone();
            self.bind(value, move |check, active| {
                guard.run(|| check.set_active(*active));
            });
        }
        let value = value.clone();
        let handler = self.widget().connect_toggled(move |check| {
            if !guard.is_syncing() {
                value.set(check.is_active());
            }
        });
        self.own_signal(handler);
    }
}

// ---------------------------------------------------------------------------
// Feedback
// ---------------------------------------------------------------------------

pub fn spinner(body: impl FnOnce(&Ctx<gtk::Spinner>)) -> Ctx<gtk::Spinner> {
    mount(gtk::Spinner::new(), body)
}

impl Ctx<gtk::Spinner> {
    pub fn bind_spinning(&self, active: impl IntoReadOnly<bool>) {
        self.bind(active, |spinner, active| spinner.set_spinning(*active));
    }
}

/// A progress bar whose fraction (0.0 to 1.0) follows `fraction`.
pub fn progress(
    fraction: impl IntoReadOnly<f64>,
    body: impl FnOnce(&Ctx<gtk::ProgressBar>),
) -> Ctx<gtk::ProgressBar> {
    mount(gtk::ProgressBar::new(), |ctx| {
        ctx.bind_fraction(fraction);
        body(ctx);
    })
}

impl Ctx<gtk::ProgressBar> {
    pub fn bind_fraction(&self, fraction: impl IntoReadOnly<f64>) {
        self.bind(fraction, |bar, fraction| bar.set_fraction(*fraction));
    }

    /// Shows `text` on the bar and keeps it current.
    pub fn bind_progress_text(&self, text: impl IntoText) {
        self.widget().set_show_text(true);
        self.bind(text.into_text(), |bar, text| {
            bar.set_text(Some(text.as_str()))
        });
    }
}

pub fn separator(orientation: gtk::Orientation) -> Ctx<gtk::Separator> {
    mount(gtk::Separator::new(orientation), |_| {})
}

/// An image showing a themed icon.
pub fn icon(name: &str, body: impl FnOnce(&Ctx<gtk::Image>)) -> Ctx<gtk::Image> {
    mount(gtk::Image::from_icon_name(name), body)
}

impl Ctx<gtk::Image> {
    pub fn bind_icon(&self, name: impl IntoText) {
        self.bind(name.into_text(), |image, name| {
            image.set_icon_name(Some(name.as_str()))
        });
    }
}

/// A button showing a themed icon instead of a label.
pub fn icon_button(name: &str, body: impl FnOnce(&Ctx<gtk::Button>)) -> Ctx<gtk::Button> {
    mount(gtk::Button::from_icon_name(name), body)
}

// ---------------------------------------------------------------------------
// Containers with one child
// ---------------------------------------------------------------------------

pub fn frame(label: Option<&str>, body: impl FnOnce(&Ctx<gtk::Frame>)) -> Ctx<gtk::Frame> {
    container(gtk::Frame::new(label), body)
}

impl Host for gtk::Button {
    fn attach(&self, child: &gtk::Widget) {
        self.set_child(Some(child));
    }
}

impl Host for gtk::Revealer {
    fn attach(&self, child: &gtk::Widget) {
        self.set_child(Some(child));
    }
}

impl Host for gtk::Overlay {
    fn attach(&self, child: &gtk::Widget) {
        self.set_child(Some(child));
    }
}

impl Host for gtk::Popover {
    fn attach(&self, child: &gtk::Widget) {
        self.set_child(Some(child));
    }
}

/// An overlay: the body is the main child, [`Ctx::layer`] adds layers on top.
pub fn overlay(body: impl FnOnce(&Ctx<gtk::Overlay>)) -> Ctx<gtk::Overlay> {
    container(gtk::Overlay::new(), body)
}

impl Ctx<gtk::Overlay> {
    /// Composes `body` as a layer above the main child.
    pub fn layer<R>(&self, body: impl FnOnce() -> R) -> R {
        let slot = OverlaySlot(self.widget().clone());
        scope::compose_into(Rc::new(slot), self.scope(), body)
    }
}

struct OverlaySlot(gtk::Overlay);

impl Host for OverlaySlot {
    fn attach(&self, child: &gtk::Widget) {
        self.0.add_overlay(child);
    }
}

// ---------------------------------------------------------------------------
// Containers with slots
// ---------------------------------------------------------------------------

/// A paned split; fill its halves with [`Ctx::start`] and [`Ctx::end`].
pub fn paned(
    orientation: gtk::Orientation,
    body: impl FnOnce(&Ctx<gtk::Paned>),
) -> Ctx<gtk::Paned> {
    mount(gtk::Paned::new(orientation), body)
}

impl Ctx<gtk::Paned> {
    /// Composes `body` as the first (left or top) child.
    pub fn start<R>(&self, body: impl FnOnce() -> R) -> R {
        let slot = PanedSlot {
            paned: self.widget().clone(),
            start: true,
        };
        scope::compose_into(Rc::new(slot), self.scope(), body)
    }

    /// Composes `body` as the second (right or bottom) child.
    pub fn end<R>(&self, body: impl FnOnce() -> R) -> R {
        let slot = PanedSlot {
            paned: self.widget().clone(),
            start: false,
        };
        scope::compose_into(Rc::new(slot), self.scope(), body)
    }
}

struct PanedSlot {
    paned: gtk::Paned,
    start: bool,
}

impl Host for PanedSlot {
    fn attach(&self, child: &gtk::Widget) {
        if self.start {
            self.paned.set_start_child(Some(child));
        } else {
            self.paned.set_end_child(Some(child));
        }
    }
}

/// A header bar; fill it with [`Ctx::pack_start`], [`Ctx::pack_end`] and [`Ctx::title`].
pub fn header_bar(body: impl FnOnce(&Ctx<gtk::HeaderBar>)) -> Ctx<gtk::HeaderBar> {
    mount(gtk::HeaderBar::new(), body)
}

impl Ctx<gtk::HeaderBar> {
    pub fn pack_start<R>(&self, body: impl FnOnce() -> R) -> R {
        self.compose_slot(HeaderSlot::Start, body)
    }

    pub fn pack_end<R>(&self, body: impl FnOnce() -> R) -> R {
        self.compose_slot(HeaderSlot::End, body)
    }

    /// Composes `body` as the title widget.
    pub fn title<R>(&self, body: impl FnOnce() -> R) -> R {
        self.compose_slot(HeaderSlot::Title, body)
    }

    fn compose_slot<R>(&self, slot: HeaderSlot, body: impl FnOnce() -> R) -> R {
        let host = HeaderHost {
            bar: self.widget().clone(),
            slot,
        };
        scope::compose_into(Rc::new(host), self.scope(), body)
    }
}

enum HeaderSlot {
    Start,
    End,
    Title,
}

struct HeaderHost {
    bar: gtk::HeaderBar,
    slot: HeaderSlot,
}

impl Host for HeaderHost {
    fn attach(&self, child: &gtk::Widget) {
        match self.slot {
            HeaderSlot::Start => self.bar.pack_start(child),
            HeaderSlot::End => self.bar.pack_end(child),
            HeaderSlot::Title => self.bar.set_title_widget(Some(child)),
        }
    }
}

// ---------------------------------------------------------------------------
// Popovers and events on any widget
// ---------------------------------------------------------------------------

struct PopoverParent(gtk::Widget);

impl Host for PopoverParent {
    fn attach(&self, child: &gtk::Widget) {
        child.set_parent(&self.0);
    }
}

impl<W: IsA<gtk::Widget>> Ctx<W> {
    /// A popover anchored to this widget. It is unparented when the scope ends,
    /// which GTK requires before the parent can be finalized.
    pub fn popover(&self, body: impl FnOnce(&Ctx<gtk::Popover>)) -> Ctx<gtk::Popover> {
        let parent = PopoverParent(self.widget().upcast_ref::<gtk::Widget>().clone());
        scope::compose_into(Rc::new(parent), self.scope(), || {
            let popover = container(gtk::Popover::new(), body);
            let widget = popover.widget().clone();
            popover.add_disposable(Disposable::new(move || widget.unparent()));
            popover
        })
    }

    /// Keeps an event controller on this widget only as long as the scope lives.
    pub fn own_controller(&self, controller: impl IsA<gtk::EventController>) {
        let widget = self.widget().clone();
        let controller = controller.upcast::<gtk::EventController>();
        widget.add_controller(controller.clone());
        self.add_disposable(Disposable::new(move || {
            widget.remove_controller(&controller)
        }));
    }

    /// Key presses on this widget. The handler returns `true` when it consumed the key.
    pub fn on_key_pressed(&self, handler: impl Fn(gdk::Key, gdk::ModifierType) -> bool + 'static) {
        let controller = gtk::EventControllerKey::new();
        controller.connect_key_pressed(move |_, key, _code, state| {
            if handler(key, state) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        self.own_controller(controller);
    }

    /// Presses of one mouse button (1 = primary, 3 = secondary) with the pointer position.
    pub fn on_pressed(&self, button: u32, handler: impl Fn(f64, f64) + 'static) {
        let gesture = gtk::GestureClick::new();
        gesture.set_button(button);
        gesture.connect_pressed(move |_, _count, x, y| handler(x, y));
        self.own_controller(gesture);
    }

    /// Runs with `true` when this widget gains keyboard focus and `false` when it loses it.
    pub fn on_focus_change(&self, handler: impl Fn(bool) + 'static) {
        let handler = self
            .widget()
            .connect_has_focus_notify(move |widget| handler(widget.has_focus()));
        self.own_signal(handler);
    }

    /// Sets the same margin on all four sides.
    pub fn margin(&self, all: i32) {
        let widget = self.widget();
        widget.set_margin_top(all);
        widget.set_margin_bottom(all);
        widget.set_margin_start(all);
        widget.set_margin_end(all);
    }
}

impl<W: IsA<gtk::Widget> + IsA<gtk::Window>> Ctx<W> {
    /// Runs when the window is asked to close. Return `true` to keep it open.
    pub fn on_close_request(&self, handler: impl Fn() -> bool + 'static) {
        let handler = self.widget().connect_close_request(move |_| {
            if handler() {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        self.own_signal(handler);
    }
}

// ---------------------------------------------------------------------------
// Reactive option lists
// ---------------------------------------------------------------------------

/// A drop-down over a reactive list of options, with the selected index bound
/// both ways to `selected`.
///
/// When the options change, the model is rebuilt and the selection is restored
/// from `selected`, so a rebuilt list never pushes a stray index back into the
/// property.
pub fn dropdown_list(
    options: &super::list::ListProperty<String>,
    selected: &Property<u32>,
    body: impl FnOnce(&Ctx<gtk::DropDown>),
) -> Ctx<gtk::DropDown> {
    let model = gtk::StringList::new(&[]);
    let dropdown = gtk::DropDown::new(Some(model.clone()), gtk::Expression::NONE);
    mount(dropdown, |ctx| {
        let guard = SyncGuard::default();
        {
            let guard = guard.clone();
            let selected = selected.clone();
            let model = model.clone();
            let dropdown = ctx.widget().clone();
            ctx.add_disposable(options.observe(move |items| {
                guard.run(|| {
                    let names = items.iter().map(String::as_str).collect::<Vec<_>>();
                    model.splice(0, model.n_items(), &names);
                    dropdown.set_selected(selected.get());
                });
            }));
        }
        {
            let guard = guard.clone();
            ctx.bind(selected, move |dropdown, index| {
                guard.run(|| dropdown.set_selected(*index));
            });
        }
        let selected = selected.clone();
        let handler = ctx.widget().connect_selected_notify(move |dropdown| {
            if !guard.is_syncing() {
                selected.set(dropdown.selected());
            }
        });
        ctx.own_signal(handler);
        body(ctx);
    })
}

// ---------------------------------------------------------------------------
// Text and scrolling
// ---------------------------------------------------------------------------

/// A text view over `buffer`.
pub fn text_view(
    buffer: &gtk::TextBuffer,
    body: impl FnOnce(&Ctx<gtk::TextView>),
) -> Ctx<gtk::TextView> {
    mount(gtk::TextView::with_buffer(buffer), body)
}

/// A scrollbar driving `adjustment`.
pub fn scrollbar(
    orientation: gtk::Orientation,
    adjustment: &gtk::Adjustment,
    body: impl FnOnce(&Ctx<gtk::Scrollbar>),
) -> Ctx<gtk::Scrollbar> {
    mount(gtk::Scrollbar::new(orientation, Some(adjustment)), body)
}
