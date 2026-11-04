//! Window-level building blocks: actions with accelerators, the titlebar
//! slot, timers, GObject properties as observable values, and composing into
//! an existing container later on.

use std::{cell::RefCell, rc::Rc, time::Duration};

use gtk::{gio, glib, prelude::*};

use super::{
    components::Ctx,
    scope::{self, Host},
    state::{Disposable, ReadOnly, Source},
};

impl<W: IsA<gtk::Widget> + Host + Clone> Ctx<W> {
    /// Composes more children into this container, in its scope.
    ///
    /// This is `renderInto` of scalajs-ui: a container built earlier can be
    /// filled later, and what is added there ends with the same scope.
    pub fn compose<R>(&self, body: impl FnOnce() -> R) -> R {
        scope::compose_into(Rc::new(self.widget().clone()), self.scope(), body)
    }
}

impl<W: IsA<gtk::Widget>> Ctx<W> {
    /// Keeps a signal handler of any object only as long as this scope lives.
    pub fn own_object_signal<O: IsA<glib::Object>>(
        &self,
        object: &O,
        handler: glib::signal::SignalHandlerId,
    ) {
        let object = object.upcast_ref::<glib::Object>().downgrade();
        self.add_disposable(Disposable::new(move || {
            if let Some(object) = object.upgrade() {
                object.disconnect(handler);
            }
        }));
    }

    /// Runs `tick` every `interval` while the scope lives.
    pub fn every(&self, interval: Duration, tick: impl Fn() + 'static) {
        let source = glib::timeout_add_local(interval, move || {
            tick();
            glib::ControlFlow::Continue
        });
        let source = RefCell::new(Some(source));
        self.add_disposable(Disposable::new(move || {
            if let Some(source) = source.take() {
                source.remove();
            }
        }));
    }

    /// Runs `run` once after `delay`, unless the scope ends first.
    pub fn after(&self, delay: Duration, run: impl FnOnce() + 'static) {
        let pending: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
        let source = {
            let pending = Rc::clone(&pending);
            glib::timeout_add_local_once(delay, move || {
                // A source that has fired is gone; it must not be removed again.
                pending.borrow_mut().take();
                run();
            })
        };
        *pending.borrow_mut() = Some(source);
        self.add_disposable(Disposable::new(move || {
            let source = pending.borrow_mut().take();
            if let Some(source) = source {
                source.remove();
            }
        }));
    }

    /// Scroll events on this widget; the handler gets the deltas and returns
    /// `true` when it consumed them.
    pub fn on_scroll(
        &self,
        flags: gtk::EventControllerScrollFlags,
        handler: impl Fn(f64, f64) -> bool + 'static,
    ) {
        let controller = gtk::EventControllerScroll::new(flags);
        controller.connect_scroll(move |_, dx, dy| {
            if handler(dx, dy) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        self.own_controller(controller);
    }
}

struct TitlebarSlot(gtk::Window);

impl Host for TitlebarSlot {
    fn attach(&self, child: &gtk::Widget) {
        self.0.set_titlebar(Some(child));
    }
}

impl<W: IsA<gtk::Widget> + IsA<gtk::Window>> Ctx<W> {
    /// Composes `body` as the window's titlebar.
    pub fn titlebar<R>(&self, body: impl FnOnce() -> R) -> R {
        let window: gtk::Window = self.widget().clone().upcast();
        scope::compose_into(Rc::new(TitlebarSlot(window)), self.scope(), body)
    }
}

impl Ctx<gtk::ApplicationWindow> {
    /// A window action `win.<name>`, triggered by `accels` and by every
    /// widget whose action name points at it.
    ///
    /// Buttons that call `set_action_name("win.<name>")` and the keyboard
    /// shortcut go through the same action, so a command exists exactly once.
    pub fn action(&self, name: &str, accels: &[&str], run: impl Fn() + 'static) {
        let window = self.widget().clone();
        let action = gio::SimpleAction::new(name, None);
        action.connect_activate(move |_, _| run());
        window.add_action(&action);
        if !accels.is_empty() {
            if let Some(application) = window.application() {
                application.set_accels_for_action(&format!("win.{name}"), accels);
            }
        }
        let name = name.to_owned();
        self.add_disposable(Disposable::new(move || window.remove_action(&name)));
    }
}

/// A GObject property as an observable value, read through `read`.
///
/// ```ignore
/// let maximized = from_notify(&window, "maximized", |w| w.is_maximized());
/// button.bind(&maximized, |b, max| b.set_icon_name(if *max { "restore" } else { "maximize" }));
/// ```
pub fn from_notify<O, T>(
    object: &O,
    property: &'static str,
    read: impl Fn(&O) -> T + 'static,
) -> ReadOnly<T>
where
    O: IsA<glib::Object>,
    T: Clone + 'static,
{
    ReadOnly::from_source(Rc::new(Notify {
        object: object.clone(),
        property,
        read: Rc::new(read),
    }))
}

type ReadFn<O, T> = Rc<dyn Fn(&O) -> T>;

struct Notify<O, T> {
    object: O,
    property: &'static str,
    read: ReadFn<O, T>,
}

impl<O: IsA<glib::Object>, T: Clone + 'static> Source<T> for Notify<O, T> {
    fn current(&self) -> T {
        (self.read)(&self.object)
    }

    fn subscribe(&self, observer: Rc<dyn Fn(&T)>, notify_initial: bool) -> Disposable {
        let read = Rc::clone(&self.read);
        let notify_observer = Rc::clone(&observer);
        let handler = self
            .object
            .connect_notify_local(Some(self.property), move |object, _| {
                notify_observer(&read(object));
            });
        if notify_initial {
            observer(&self.current());
        }
        let object = self.object.upcast_ref::<glib::Object>().downgrade();
        Disposable::new(move || {
            if let Some(object) = object.upgrade() {
                object.disconnect(handler);
            }
        })
    }
}
