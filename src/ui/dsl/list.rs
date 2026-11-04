//! `ListProperty`: an observable list with structural change events, the
//! counterpart of `ui.core.state.ListProperty` in scalajs-ui.
//!
//! Observers hear every change as a [`ListChange`], which is what lets
//! `foreach` touch only the affected children instead of rebuilding them all.

use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

use super::state::{Disposable, ReadOnly, Source};

/// One structural change of a [`ListProperty`].
#[derive(Clone, Debug, PartialEq)]
pub enum ListChange<T> {
    /// The whole content was replaced by `items`.
    Reset {
        items: Vec<T>,
    },
    Insert {
        index: usize,
        element: T,
    },
    Remove {
        index: usize,
        element: T,
    },
    Update {
        index: usize,
        old: T,
        new: T,
    },
    Clear,
}

type ChangeObserver<T> = Rc<dyn Fn(&ListChange<T>)>;
type ContentObserver<T> = Rc<dyn Fn(&[T])>;

struct Listener<T> {
    id: u64,
    active: Rc<Cell<bool>>,
    observer: ChangeObserver<T>,
}

struct ListInner<T> {
    items: RefCell<Vec<T>>,
    listeners: RefCell<Vec<Listener<T>>>,
    next_id: Cell<u64>,
    notifying: Cell<bool>,
}

/// An observable list. Cloning clones the handle; all clones share one list.
pub struct ListProperty<T>(Rc<ListInner<T>>);

impl<T> Clone for ListProperty<T> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<T: Clone + 'static> Default for ListProperty<T> {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl<T: Clone + 'static> ListProperty<T> {
    pub fn new(items: Vec<T>) -> Self {
        Self(Rc::new(ListInner {
            items: RefCell::new(items),
            listeners: RefCell::new(Vec::new()),
            next_id: Cell::new(0),
            notifying: Cell::new(false),
        }))
    }

    pub fn get(&self) -> Vec<T> {
        self.0.items.borrow().clone()
    }

    pub fn len(&self) -> usize {
        self.0.items.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.items.borrow().is_empty()
    }

    pub fn item(&self, index: usize) -> Option<T> {
        self.0.items.borrow().get(index).cloned()
    }

    pub fn push(&self, element: T) {
        let index = self.len();
        self.insert(index, element);
    }

    /// # Panics
    ///
    /// When `index` is past the end of the list.
    pub fn insert(&self, index: usize, element: T) {
        self.0.items.borrow_mut().insert(index, element.clone());
        self.notify(ListChange::Insert { index, element });
    }

    /// # Panics
    ///
    /// When `index` is out of bounds.
    pub fn remove(&self, index: usize) -> T {
        let element = self.0.items.borrow_mut().remove(index);
        self.notify(ListChange::Remove {
            index,
            element: element.clone(),
        });
        element
    }

    /// Replaces one element and notifies observers when it differs from the current one.
    ///
    /// # Panics
    ///
    /// When `index` is out of bounds.
    pub fn set(&self, index: usize, element: T)
    where
        T: PartialEq,
    {
        let old = {
            let mut items = self.0.items.borrow_mut();
            let slot = items
                .get_mut(index)
                .unwrap_or_else(|| panic!("ui::dsl: list index {index} out of bounds"));
            if *slot == element {
                return;
            }
            std::mem::replace(slot, element.clone())
        };
        self.notify(ListChange::Update {
            index,
            old,
            new: element,
        });
    }

    pub fn clear(&self) {
        if self.is_empty() {
            return;
        }
        self.0.items.borrow_mut().clear();
        self.notify(ListChange::Clear);
    }

    /// Replaces the whole content; observers hear one `Reset`.
    pub fn replace_all(&self, items: Vec<T>) {
        *self.0.items.borrow_mut() = items.clone();
        self.notify(ListChange::Reset { items });
    }

    /// Hears every structural change, not the current content.
    pub fn observe_changes(&self, observer: impl Fn(&ListChange<T>) + 'static) -> Disposable {
        self.subscribe(Rc::new(observer))
    }

    /// Hears the current content now and the full content after every change.
    pub fn observe(&self, observer: impl Fn(&[T]) + 'static) -> Disposable {
        let observer: ContentObserver<T> = Rc::new(observer);
        let weak = Rc::downgrade(&self.0);
        let subscription = {
            let observer = Rc::clone(&observer);
            self.subscribe(Rc::new(move |_change: &ListChange<T>| {
                if let Some(inner) = weak.upgrade() {
                    let snapshot = inner.items.borrow().clone();
                    observer(&snapshot);
                }
            }))
        };
        let snapshot = self.get();
        observer(&snapshot);
        subscription
    }

    /// The list as one observable value, for example to derive a count.
    pub fn read_only(&self) -> ReadOnly<Vec<T>> {
        ReadOnly::from_source(Rc::new(self.clone()))
    }

    pub fn map<U: Clone + 'static>(&self, transform: impl Fn(&[T]) -> U + 'static) -> ReadOnly<U> {
        self.read_only().map(move |items| transform(items))
    }

    fn notify(&self, change: ListChange<T>) {
        let inner = &self.0;
        if inner.notifying.replace(true) {
            panic!("ui::dsl: list propagation cycle detected while notifying observers");
        }
        let listeners: Vec<(Rc<Cell<bool>>, ChangeObserver<T>)> = inner
            .listeners
            .borrow()
            .iter()
            .map(|listener| (Rc::clone(&listener.active), Rc::clone(&listener.observer)))
            .collect();
        for (active, observer) in listeners {
            if active.get() {
                observer(&change);
            }
        }
        inner.notifying.set(false);
    }

    fn subscribe(&self, observer: ChangeObserver<T>) -> Disposable {
        let inner = &self.0;
        let id = inner.next_id.get();
        inner.next_id.set(id + 1);
        let active = Rc::new(Cell::new(true));
        inner.listeners.borrow_mut().push(Listener {
            id,
            active: Rc::clone(&active),
            observer,
        });
        let weak: Weak<ListInner<T>> = Rc::downgrade(&self.0);
        Disposable::new(move || {
            active.set(false);
            if let Some(inner) = weak.upgrade() {
                inner
                    .listeners
                    .borrow_mut()
                    .retain(|listener| listener.id != id);
            }
        })
    }
}

impl<T: Clone + 'static> Source<Vec<T>> for ListProperty<T> {
    fn current(&self) -> Vec<T> {
        self.get()
    }

    fn subscribe(&self, observer: Rc<dyn Fn(&Vec<T>)>, notify_initial: bool) -> Disposable {
        let weak = Rc::downgrade(&self.0);
        let subscription = {
            let observer = Rc::clone(&observer);
            ListProperty::subscribe(
                self,
                Rc::new(move |_change: &ListChange<T>| {
                    if let Some(inner) = weak.upgrade() {
                        let snapshot = inner.items.borrow().clone();
                        observer(&snapshot);
                    }
                }),
            )
        };
        if notify_initial {
            let snapshot = self.get();
            observer(&snapshot);
        }
        subscription
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/ui_dsl_list_tests.rs"]
mod tests;
