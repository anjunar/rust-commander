//! Reactive state for the UI DSL.
//!
//! Mirrors `ui.core.state` of scalajs-ui: a [`Property`] holds a value and
//! notifies its observers synchronously, a [`ReadOnly`] is a derived or
//! constant view of one, and every subscription is a [`Disposable`] that the
//! owning scope releases when its widgets go away.

use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

/// A piece of cleanup that runs at most once.
#[derive(Clone)]
pub struct Disposable(Rc<DisposableInner>);

struct DisposableInner {
    action: RefCell<Option<Box<dyn FnOnce()>>>,
}

impl Disposable {
    pub fn new(action: impl FnOnce() + 'static) -> Self {
        Self(Rc::new(DisposableInner {
            action: RefCell::new(Some(Box::new(action))),
        }))
    }

    /// A disposable with nothing to release.
    pub fn empty() -> Self {
        Self(Rc::new(DisposableInner {
            action: RefCell::new(None),
        }))
    }

    pub fn dispose(&self) {
        let action = self.0.action.borrow_mut().take();
        if let Some(action) = action {
            action();
        }
    }

    pub fn is_disposed(&self) -> bool {
        self.0.action.borrow().is_none()
    }

    fn is_same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// Owns a set of disposables and releases them together.
///
/// Adding to a composite that is already disposed releases the item at once,
/// so late subscriptions can never outlive their scope.
#[derive(Default)]
pub struct CompositeDisposable {
    items: RefCell<Vec<Disposable>>,
    disposed: Cell<bool>,
}

impl CompositeDisposable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&self, disposable: Disposable) {
        if self.disposed.get() {
            disposable.dispose();
        } else {
            self.items.borrow_mut().push(disposable);
        }
    }

    /// Takes a registration back without disposing it.
    pub fn remove(&self, disposable: &Disposable) {
        self.items
            .borrow_mut()
            .retain(|item| !item.is_same(disposable));
    }

    pub fn dispose(&self) {
        if self.disposed.replace(true) {
            return;
        }
        let current = std::mem::take(&mut *self.items.borrow_mut());
        for item in current {
            item.dispose();
        }
    }

    pub fn is_disposed(&self) -> bool {
        self.disposed.get()
    }

    pub fn len(&self) -> usize {
        self.items.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.borrow().is_empty()
    }
}

type Observer<T> = Rc<dyn Fn(&T)>;

/// Object-safe core shared by [`Property`] and every [`ReadOnly`] view.
pub(crate) trait Source<T> {
    fn current(&self) -> T;

    fn subscribe(&self, observer: Observer<T>, notify_initial: bool) -> Disposable;
}

struct Listener<T> {
    id: u64,
    active: Rc<Cell<bool>>,
    observer: Observer<T>,
}

struct PropertyInner<T> {
    value: RefCell<T>,
    default: RefCell<T>,
    listeners: RefCell<Vec<Listener<T>>>,
    next_id: Cell<u64>,
    notifying: Cell<bool>,
}

/// A writable, observable value.
///
/// Cloning a property clones the handle, not the value: all clones share one
/// state, which is what lets widgets and handlers refer to the same model.
pub struct Property<T>(Rc<PropertyInner<T>>);

impl<T> Clone for Property<T> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<T: Clone + 'static> Property<T> {
    pub fn new(value: T) -> Self {
        Self::with_default(value.clone(), value)
    }

    /// A property whose [`reset`](Self::reset) target differs from its initial value.
    pub fn with_default(value: T, default: T) -> Self {
        Self(Rc::new(PropertyInner {
            value: RefCell::new(value),
            default: RefCell::new(default),
            listeners: RefCell::new(Vec::new()),
            next_id: Cell::new(0),
            notifying: Cell::new(false),
        }))
    }

    pub fn get(&self) -> T {
        self.0.value.borrow().clone()
    }

    /// Stores the value and notifies observers when it differs from the current one.
    pub fn set(&self, value: T)
    where
        T: PartialEq,
    {
        let changed = *self.0.value.borrow() != value;
        if changed {
            self.set_always(value);
        }
    }

    /// Stores the value and notifies observers even when it is unchanged.
    ///
    /// # Panics
    ///
    /// When called from one of this property's own observers: that is a
    /// propagation cycle, the same programming error scalajs-ui reports.
    pub fn set_always(&self, value: T) {
        let inner = &self.0;
        if inner.notifying.replace(true) {
            panic!("ui::dsl: property propagation cycle detected while notifying observers");
        }
        *inner.value.borrow_mut() = value;
        let snapshot = inner.value.borrow().clone();
        let listeners: Vec<(Rc<Cell<bool>>, Observer<T>)> = inner
            .listeners
            .borrow()
            .iter()
            .map(|listener| (Rc::clone(&listener.active), Rc::clone(&listener.observer)))
            .collect();
        for (active, observer) in listeners {
            // Another observer can dispose this subscription within the same notification.
            if active.get() {
                observer(&snapshot);
            }
        }
        inner.notifying.set(false);
    }

    /// Derives the next value from the current one.
    pub fn update(&self, transform: impl FnOnce(&T) -> T)
    where
        T: PartialEq,
    {
        let next = transform(&self.get());
        self.set(next);
    }

    pub fn set_default(&self, value: T) {
        *self.0.default.borrow_mut() = value;
    }

    pub fn is_dirty(&self) -> bool
    where
        T: PartialEq,
    {
        *self.0.value.borrow() != *self.0.default.borrow()
    }

    /// Back to the value the property was built with, or the one last given to `set_default`.
    pub fn reset(&self)
    where
        T: PartialEq,
    {
        let default = self.0.default.borrow().clone();
        self.set(default);
    }

    /// Runs the observer with the current value now and with every change.
    pub fn observe(&self, observer: impl Fn(&T) + 'static) -> Disposable {
        self.subscribe(Rc::new(observer), true)
    }

    /// Runs the observer with every change, not with the current value.
    pub fn observe_without_initial(&self, observer: impl Fn(&T) + 'static) -> Disposable {
        self.subscribe(Rc::new(observer), false)
    }

    pub fn read_only(&self) -> ReadOnly<T> {
        ReadOnly(Rc::new(self.clone()))
    }

    pub fn map<U: Clone + 'static>(&self, transform: impl Fn(&T) -> U + 'static) -> ReadOnly<U> {
        self.read_only().map(transform)
    }

    pub fn combine<U: Clone + 'static, R: Clone + 'static>(
        &self,
        other: &ReadOnly<U>,
        combine: impl Fn(&T, &U) -> R + 'static,
    ) -> ReadOnly<R> {
        self.read_only().combine(other, combine)
    }

    fn subscribe(&self, observer: Observer<T>, notify_initial: bool) -> Disposable {
        let inner = &self.0;
        let id = inner.next_id.get();
        inner.next_id.set(id + 1);
        let active = Rc::new(Cell::new(true));
        inner.listeners.borrow_mut().push(Listener {
            id,
            active: Rc::clone(&active),
            observer: Rc::clone(&observer),
        });

        // The subscription must not keep the property alive: a scope that outlives
        // its model would otherwise pin the model for as long as the widgets exist.
        let weak: Weak<PropertyInner<T>> = Rc::downgrade(&self.0);
        let disposable = Disposable::new(move || {
            active.set(false);
            if let Some(inner) = weak.upgrade() {
                inner
                    .listeners
                    .borrow_mut()
                    .retain(|listener| listener.id != id);
            }
        });

        if notify_initial {
            let value = self.get();
            observer(&value);
        }
        disposable
    }
}

impl<T: Clone + 'static> Source<T> for Property<T> {
    fn current(&self) -> T {
        self.get()
    }

    fn subscribe(&self, observer: Observer<T>, notify_initial: bool) -> Disposable {
        Property::subscribe(self, observer, notify_initial)
    }
}

/// A readable, observable value: a property, a mapped view of one, or a constant.
pub struct ReadOnly<T>(Rc<dyn Source<T>>);

impl<T> Clone for ReadOnly<T> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<T: Clone + 'static> ReadOnly<T> {
    /// A value that never changes; observers hear it exactly once.
    pub fn constant(value: T) -> Self {
        Self(Rc::new(Constant(value)))
    }

    pub(crate) fn from_source(source: Rc<dyn Source<T>>) -> Self {
        Self(source)
    }

    pub fn get(&self) -> T {
        self.0.current()
    }

    pub fn observe(&self, observer: impl Fn(&T) + 'static) -> Disposable {
        self.0.subscribe(Rc::new(observer), true)
    }

    pub fn observe_without_initial(&self, observer: impl Fn(&T) + 'static) -> Disposable {
        self.0.subscribe(Rc::new(observer), false)
    }

    pub fn map<U: Clone + 'static>(&self, transform: impl Fn(&T) -> U + 'static) -> ReadOnly<U> {
        ReadOnly(Rc::new(Mapped {
            source: self.clone(),
            transform: Rc::new(transform),
        }))
    }

    /// A value derived from this one and `other`, recomputed when either changes.
    pub fn combine<U: Clone + 'static, R: Clone + 'static>(
        &self,
        other: &ReadOnly<U>,
        combine: impl Fn(&T, &U) -> R + 'static,
    ) -> ReadOnly<R> {
        ReadOnly(Rc::new(Combined {
            first: self.clone(),
            second: other.clone(),
            combine: Rc::new(combine),
        }))
    }
}

struct Constant<T>(T);

impl<T: Clone> Source<T> for Constant<T> {
    fn current(&self) -> T {
        self.0.clone()
    }

    fn subscribe(&self, observer: Observer<T>, notify_initial: bool) -> Disposable {
        if notify_initial {
            observer(&self.0);
        }
        Disposable::empty()
    }
}

struct Mapped<T, U> {
    source: ReadOnly<T>,
    transform: Rc<dyn Fn(&T) -> U>,
}

impl<T: Clone + 'static, U: Clone + 'static> Source<U> for Mapped<T, U> {
    fn current(&self) -> U {
        (self.transform)(&self.source.get())
    }

    fn subscribe(&self, observer: Observer<U>, notify_initial: bool) -> Disposable {
        let transform = Rc::clone(&self.transform);
        let mapped: Observer<T> = Rc::new(move |value: &T| {
            let next = transform(value);
            observer(&next);
        });
        self.source.0.subscribe(mapped, notify_initial)
    }
}

type CombineFn<A, B, R> = Rc<dyn Fn(&A, &B) -> R>;

struct Combined<A, B, R> {
    first: ReadOnly<A>,
    second: ReadOnly<B>,
    combine: CombineFn<A, B, R>,
}

impl<A: Clone + 'static, B: Clone + 'static, R: Clone + 'static> Source<R> for Combined<A, B, R> {
    fn current(&self) -> R {
        (self.combine)(&self.first.get(), &self.second.get())
    }

    fn subscribe(&self, observer: Observer<R>, notify_initial: bool) -> Disposable {
        let on_first = {
            let second = self.second.clone();
            let combine = Rc::clone(&self.combine);
            let observer = Rc::clone(&observer);
            self.first.0.subscribe(
                Rc::new(move |first: &A| observer(&combine(first, &second.get()))),
                false,
            )
        };
        let on_second = {
            let first = self.first.clone();
            let combine = Rc::clone(&self.combine);
            let observer = Rc::clone(&observer);
            self.second.0.subscribe(
                Rc::new(move |second: &B| observer(&combine(&first.get(), second))),
                false,
            )
        };
        if notify_initial {
            observer(&self.current());
        }
        Disposable::new(move || {
            on_first.dispose();
            on_second.dispose();
        })
    }
}

/// Anything the DSL can observe: a `Property`, a `ReadOnly`, or a reference to either.
pub trait IntoReadOnly<T> {
    fn into_read_only(self) -> ReadOnly<T>;
}

impl<T: Clone + 'static> IntoReadOnly<T> for ReadOnly<T> {
    fn into_read_only(self) -> ReadOnly<T> {
        self
    }
}

impl<T: Clone + 'static> IntoReadOnly<T> for &ReadOnly<T> {
    fn into_read_only(self) -> ReadOnly<T> {
        self.clone()
    }
}

impl<T: Clone + 'static> IntoReadOnly<T> for Property<T> {
    fn into_read_only(self) -> ReadOnly<T> {
        self.read_only()
    }
}

impl<T: Clone + 'static> IntoReadOnly<T> for &Property<T> {
    fn into_read_only(self) -> ReadOnly<T> {
        self.read_only()
    }
}

/// Text for a label or button: a plain string or a reactive string.
///
/// This is scalajs-ui's `TextValue`: `text("Hello")` and
/// `text(count.map(|n| format!("{n}")))` go through the same function.
pub trait IntoText {
    fn into_text(self) -> ReadOnly<String>;
}

impl IntoText for &str {
    fn into_text(self) -> ReadOnly<String> {
        ReadOnly::constant(self.to_owned())
    }
}

impl IntoText for String {
    fn into_text(self) -> ReadOnly<String> {
        ReadOnly::constant(self)
    }
}

impl IntoText for &String {
    fn into_text(self) -> ReadOnly<String> {
        ReadOnly::constant(self.clone())
    }
}

impl IntoText for Cow<'_, str> {
    fn into_text(self) -> ReadOnly<String> {
        ReadOnly::constant(self.into_owned())
    }
}

impl IntoText for ReadOnly<String> {
    fn into_text(self) -> ReadOnly<String> {
        self
    }
}

impl IntoText for &ReadOnly<String> {
    fn into_text(self) -> ReadOnly<String> {
        self.clone()
    }
}

impl IntoText for Property<String> {
    fn into_text(self) -> ReadOnly<String> {
        self.read_only()
    }
}

impl IntoText for &Property<String> {
    fn into_text(self) -> ReadOnly<String> {
        self.read_only()
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/ui_dsl_state_tests.rs"]
mod tests;
