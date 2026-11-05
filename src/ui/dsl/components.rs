//! Elements and bindings of the DSL.
//!
//! Each element function creates a GTK widget, mounts it into the current
//! parent, and runs its body with a [`Ctx`] handle. The handle dereferences to
//! the widget, so every GTK method stays available, and adds the reactive
//! bindings whose lifetime is tied to the enclosing scope.

use std::{
    cell::{Cell, RefCell},
    ops::Deref,
    rc::Rc,
};

use gtk::{glib, prelude::*};

use super::{
    scope::{self, Host, Scope},
    state::{CompositeDisposable, Disposable, IntoReadOnly, IntoText, Property},
};

/// A mounted widget together with the scope its bindings belong to.
pub struct Ctx<W: IsA<gtk::Widget>> {
    widget: W,
    scope: Scope,
}

impl<W: IsA<gtk::Widget>> Clone for Ctx<W> {
    fn clone(&self) -> Self {
        Self {
            widget: self.widget.clone(),
            scope: Rc::clone(&self.scope),
        }
    }
}

impl<W: IsA<gtk::Widget>> Deref for Ctx<W> {
    type Target = W;

    fn deref(&self) -> &W {
        &self.widget
    }
}

impl<W: IsA<gtk::Widget>> Ctx<W> {
    pub fn widget(&self) -> &W {
        &self.widget
    }

    pub fn scope(&self) -> &Scope {
        &self.scope
    }

    /// Ties cleanup to this widget's scope.
    pub fn add_disposable(&self, disposable: Disposable) {
        self.scope.add(disposable);
    }

    /// Keeps a signal handler only as long as the scope lives.
    pub fn own_signal(&self, handler: glib::signal::SignalHandlerId) {
        let widget = self.widget.clone();
        self.add_disposable(Disposable::new(move || widget.disconnect(handler)));
    }

    /// Runs `apply` with the current value now and again on every change.
    pub fn bind<T: Clone + 'static>(
        &self,
        source: impl IntoReadOnly<T>,
        apply: impl Fn(&W, &T) + 'static,
    ) {
        let widget = self.widget.clone();
        self.add_disposable(
            source
                .into_read_only()
                .observe(move |value| apply(&widget, value)),
        );
    }

    pub fn css(&self, class: &str) {
        self.widget.add_css_class(class);
    }

    /// Adds the CSS class while the condition holds, removes it otherwise.
    pub fn css_if(&self, class: &str, condition: impl IntoReadOnly<bool>) {
        let class = class.to_owned();
        self.bind(condition, move |widget, active| {
            if *active {
                widget.add_css_class(&class);
            } else {
                widget.remove_css_class(&class);
            }
        });
    }

    pub fn bind_sensitive(&self, condition: impl IntoReadOnly<bool>) {
        self.bind(condition, |widget, sensitive| {
            widget.set_sensitive(*sensitive)
        });
    }

    pub fn bind_visible(&self, condition: impl IntoReadOnly<bool>) {
        self.bind(condition, |widget, visible| widget.set_visible(*visible));
    }

    pub fn bind_tooltip(&self, text: impl IntoText) {
        self.bind(text.into_text(), |widget, text| {
            widget.set_tooltip_text(Some(text.as_str()));
        });
    }
}

impl Ctx<gtk::Label> {
    pub fn bind_text(&self, text: impl IntoText) {
        self.bind(text.into_text(), |label, text| label.set_label(text));
    }
}

impl Ctx<gtk::Button> {
    pub fn on_click(&self, handler: impl Fn() + 'static) {
        let handler = self.widget.connect_clicked(move |_| handler());
        self.own_signal(handler);
    }

    pub fn bind_label(&self, text: impl IntoText) {
        self.bind(text.into_text(), |button, text| button.set_label(text));
    }
}

impl Ctx<gtk::Switch> {
    /// Two-way binding between the switch and a boolean property.
    pub fn bind_active(&self, value: &Property<bool>) {
        let guard = SyncGuard::default();
        {
            let guard = guard.clone();
            self.bind(value, move |switch, active| {
                guard.run(|| switch.set_active(*active));
            });
        }
        let value = value.clone();
        let handler = self.widget.connect_active_notify(move |switch| {
            if !guard.is_syncing() {
                value.set(switch.is_active());
            }
        });
        self.own_signal(handler);
    }
}

impl Ctx<gtk::Entry> {
    /// Two-way binding between the entry text and a string property.
    pub fn bind_text(&self, value: &Property<String>) {
        let guard = SyncGuard::default();
        {
            let guard = guard.clone();
            self.bind(value, move |entry, text| {
                // `set_text` replaces the content and puts the cursor at the
                // start, so what the user just typed must not come back here.
                if entry.text().as_str() != text.as_str() {
                    guard.run(|| entry.set_text(text));
                }
            });
        }
        let value = value.clone();
        let handler = self.widget.connect_changed(move |entry| {
            if !guard.is_syncing() {
                value.set(entry.text().to_string());
            }
        });
        self.own_signal(handler);
    }

    /// Runs when Enter is pressed in the entry.
    pub fn on_activate(&self, handler: impl Fn() + 'static) {
        let handler = self.widget.connect_activate(move |_| handler());
        self.own_signal(handler);
    }
}

impl Ctx<gtk::DropDown> {
    /// Two-way binding between the selected index and a property.
    pub fn bind_selected(&self, value: &Property<u32>) {
        let guard = SyncGuard::default();
        {
            let guard = guard.clone();
            self.bind(value, move |dropdown, selected| {
                guard.run(|| dropdown.set_selected(*selected));
            });
        }
        let value = value.clone();
        let handler = self.widget.connect_selected_notify(move |dropdown| {
            if !guard.is_syncing() {
                value.set(dropdown.selected());
            }
        });
        self.own_signal(handler);
    }
}

impl Ctx<gtk::SpinButton> {
    /// Two-way binding between the spin button and a numeric property.
    pub fn bind_value(&self, value: &Property<f64>) {
        let guard = SyncGuard::default();
        {
            let guard = guard.clone();
            self.bind(value, move |spin, number| {
                guard.run(|| spin.set_value(*number));
            });
        }
        let value = value.clone();
        let handler = self.widget.connect_value_changed(move |spin| {
            if !guard.is_syncing() {
                value.set(spin.value());
            }
        });
        self.own_signal(handler);
    }
}

/// Stops a two-way binding from echoing a widget change back into the widget.
#[derive(Clone, Default)]
pub(super) struct SyncGuard(Rc<Cell<bool>>);

impl SyncGuard {
    pub(super) fn run(&self, write: impl FnOnce()) {
        self.0.set(true);
        write();
        self.0.set(false);
    }

    pub(super) fn is_syncing(&self) -> bool {
        self.0.get()
    }
}

/// Mounts any widget into the current parent and runs its body.
///
/// This is `DslLayer.child` of scalajs-ui, and the extension point for widgets
/// the DSL has no element function for yet.
pub fn mount<W: IsA<gtk::Widget>>(widget: W, body: impl FnOnce(&Ctx<W>)) -> Ctx<W> {
    let (host, scope) = scope::current();
    host.attach(widget.upcast_ref());
    let ctx = Ctx { widget, scope };
    body(&ctx);
    ctx
}

/// Brings an existing widget into the current scope without mounting it, so
/// bindings and handlers on it end with the scope.
///
/// This is `renderInto` of scalajs-ui for widgets the DSL did not create,
/// such as the window a dialog is composed into.
pub fn adopt<W: IsA<gtk::Widget>>(widget: &W) -> Ctx<W> {
    let (_, scope) = scope::current();
    Ctx {
        widget: widget.clone(),
        scope,
    }
}

/// Mounts a widget that takes children, and composes the body into it.
pub fn container<W>(widget: W, body: impl FnOnce(&Ctx<W>)) -> Ctx<W>
where
    W: IsA<gtk::Widget> + Host,
{
    let (host, scope) = scope::current();
    host.attach(widget.upcast_ref());
    let ctx = Ctx { widget, scope };
    scope::compose_into(Rc::new(ctx.widget.clone()), &ctx.scope, || body(&ctx));
    ctx
}

pub fn vbox(spacing: i32, body: impl FnOnce(&Ctx<gtk::Box>)) -> Ctx<gtk::Box> {
    container(gtk::Box::new(gtk::Orientation::Vertical, spacing), body)
}

pub fn hbox(spacing: i32, body: impl FnOnce(&Ctx<gtk::Box>)) -> Ctx<gtk::Box> {
    container(gtk::Box::new(gtk::Orientation::Horizontal, spacing), body)
}

pub fn scrolled(body: impl FnOnce(&Ctx<gtk::ScrolledWindow>)) -> Ctx<gtk::ScrolledWindow> {
    container(gtk::ScrolledWindow::new(), body)
}

/// A left-aligned label whose text follows `value`.
pub fn text(value: impl IntoText, body: impl FnOnce(&Ctx<gtk::Label>)) -> Ctx<gtk::Label> {
    let label = gtk::Label::new(None);
    label.set_xalign(0.0);
    mount(label, |ctx| {
        ctx.bind_text(value);
        body(ctx);
    })
}

pub fn button(label: impl IntoText, body: impl FnOnce(&Ctx<gtk::Button>)) -> Ctx<gtk::Button> {
    mount(gtk::Button::new(), |ctx| {
        ctx.bind_label(label);
        body(ctx);
    })
}

/// A switch bound both ways to `value`.
pub fn switch(value: &Property<bool>, body: impl FnOnce(&Ctx<gtk::Switch>)) -> Ctx<gtk::Switch> {
    mount(gtk::Switch::new(), |ctx| {
        ctx.bind_active(value);
        body(ctx);
    })
}

/// A single-line text entry bound both ways to `value`.
pub fn entry(value: &Property<String>, body: impl FnOnce(&Ctx<gtk::Entry>)) -> Ctx<gtk::Entry> {
    mount(gtk::Entry::new(), |ctx| {
        ctx.bind_text(value);
        body(ctx);
    })
}

/// A drop-down over fixed options whose selected index is bound both ways to `selected`.
pub fn dropdown(
    options: &[&str],
    selected: &Property<u32>,
    body: impl FnOnce(&Ctx<gtk::DropDown>),
) -> Ctx<gtk::DropDown> {
    let model = gtk::StringList::new(options);
    let dropdown = gtk::DropDown::new(Some(model), gtk::Expression::NONE);
    mount(dropdown, |ctx| {
        ctx.bind_selected(selected);
        body(ctx);
    })
}

/// A spin button over `min..=max` bound both ways to `value`.
pub fn spin(
    value: &Property<f64>,
    min: f64,
    max: f64,
    step: f64,
    body: impl FnOnce(&Ctx<gtk::SpinButton>),
) -> Ctx<gtk::SpinButton> {
    let adjustment = gtk::Adjustment::new(value.get(), min, max, step, step * 10.0, 0.0);
    let spin = gtk::SpinButton::new(Some(&adjustment), 1.0, 0);
    mount(spin, |ctx| {
        ctx.bind_value(value);
        body(ctx);
    })
}

struct Mounted {
    scope: Scope,
    release: Disposable,
}

/// Composes `body` while `condition` holds and tears it down otherwise.
///
/// This is `Condition.when` of scalajs-ui: the body gets a scope of its own,
/// so its bindings end when the condition turns false, not only when the
/// surrounding render root goes away.
pub fn when(condition: impl IntoReadOnly<bool>, body: impl Fn() + 'static) -> Ctx<gtk::Box> {
    let anchor = mount(gtk::Box::new(gtk::Orientation::Vertical, 0), |_| {});
    let mounted: Rc<RefCell<Option<Mounted>>> = Rc::new(RefCell::new(None));
    let host = anchor.widget.clone();
    let parent_scope = Rc::downgrade(&anchor.scope);

    anchor.add_disposable(condition.into_read_only().observe(move |enabled| {
        let is_mounted = mounted.borrow().is_some();
        if *enabled {
            if !is_mounted {
                let scope: Scope = Rc::new(CompositeDisposable::new());
                let release = {
                    let scope = Rc::clone(&scope);
                    Disposable::new(move || scope.dispose())
                };
                if let Some(parent) = parent_scope.upgrade() {
                    parent.add(release.clone());
                }
                scope::compose_into(Rc::new(host.clone()), &scope, &body);
                *mounted.borrow_mut() = Some(Mounted { scope, release });
            }
            host.set_visible(true);
        } else {
            let previous = mounted.borrow_mut().take();
            if let Some(previous) = previous {
                if let Some(parent) = parent_scope.upgrade() {
                    parent.remove(&previous.release);
                }
                previous.scope.dispose();
                while let Some(child) = host.first_child() {
                    host.remove(&child);
                }
            }
            host.set_visible(false);
        }
    }));

    anchor
}

#[cfg(test)]
#[path = "../../../tests/unit/ui_dsl_components_tests.rs"]
mod tests;
