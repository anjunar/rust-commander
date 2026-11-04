//! Composition context: the widget that children are mounted into, and the
//! scope that owns the subscriptions and signal handlers created meanwhile.
//!
//! scalajs-ui passes both as Scala 3 context parameters (`using
//! AbstractComponent, Cursor`). Rust has no implicit parameters, so the DSL
//! keeps a thread-local stack instead. GTK is single-threaded, which makes the
//! thread-local the right scope: everything composes on the main loop.

use std::{cell::RefCell, rc::Rc};

use gtk::prelude::*;

use super::state::CompositeDisposable;

/// The disposables of one render root, or of one `when` body.
pub type Scope = Rc<CompositeDisposable>;

/// A widget that can take DSL children.
pub trait Host: 'static {
    fn attach(&self, child: &gtk::Widget);
}

impl Host for gtk::Box {
    fn attach(&self, child: &gtk::Widget) {
        self.append(child);
    }
}

impl Host for gtk::Window {
    fn attach(&self, child: &gtk::Widget) {
        self.set_child(Some(child));
    }
}

impl Host for gtk::ApplicationWindow {
    fn attach(&self, child: &gtk::Widget) {
        self.set_child(Some(child));
    }
}

impl Host for gtk::ScrolledWindow {
    fn attach(&self, child: &gtk::Widget) {
        self.set_child(Some(child));
    }
}

impl Host for gtk::Frame {
    fn attach(&self, child: &gtk::Widget) {
        self.set_child(Some(child));
    }
}

struct Frame {
    host: Rc<dyn Host>,
    scope: Scope,
}

thread_local! {
    static STACK: RefCell<Vec<Frame>> = const { RefCell::new(Vec::new()) };
}

/// Composes `body` into `host`.
///
/// Every element created inside `body` becomes a child of `host`, and every
/// subscription or signal handler created inside it belongs to one scope.
/// Releasing the scope disconnects the handlers and drops the subscriptions,
/// so the reference cycles between widgets, closures and properties are
/// broken and GTK can free the tree.
///
/// The scope is released when `host` is unrealized: GTK does that when the
/// window is destroyed (`close`, `destroy`) or when the root is removed from
/// its parent. Hiding a window only unmaps it, so its bindings stay alive.
/// A root that was never realized keeps its scope until the process ends.
///
/// Returns what `body` returns, so a handle created inside can be kept:
///
/// ```ignore
/// let save = render(&actions, || button("Save", |b| b.css("suggested-action")));
/// window.set_default_widget(Some(save.widget()));
/// ```
pub fn render<H, R>(host: &H, body: impl FnOnce() -> R) -> R
where
    H: Host + IsA<gtk::Widget>,
{
    let scope: Scope = Rc::new(CompositeDisposable::new());
    {
        let scope = Rc::clone(&scope);
        host.connect_unrealize(move |_| scope.dispose());
    }
    compose_into(Rc::new(host.clone()), &scope, body)
}

/// Runs `body` with `host` as the current parent and `scope` as the current scope.
pub(super) fn compose_into<R>(host: Rc<dyn Host>, scope: &Scope, body: impl FnOnce() -> R) -> R {
    STACK.with(|stack| {
        stack.borrow_mut().push(Frame {
            host,
            scope: Rc::clone(scope),
        })
    });
    let _guard = PopGuard;
    body()
}

struct PopGuard;

impl Drop for PopGuard {
    fn drop(&mut self) {
        STACK.with(|stack| {
            stack.borrow_mut().pop();
        });
    }
}

/// The parent and scope that DSL elements are currently composed into.
///
/// # Panics
///
/// Outside of [`render`]: there is nothing to mount into, which is the same
/// error as using a scalajs-ui element without `render` in scope.
pub(super) fn current() -> (Rc<dyn Host>, Scope) {
    STACK.with(|stack| {
        let stack = stack.borrow();
        let frame = stack
            .last()
            .expect("ui::dsl: a DSL element was created outside of `render`");
        (Rc::clone(&frame.host), Rc::clone(&frame.scope))
    })
}
