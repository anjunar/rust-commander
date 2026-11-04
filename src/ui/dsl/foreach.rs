//! `foreach`: one composed body per list element, kept in step with the list.
//!
//! The counterpart of `ui.core.statement.Foreach` in scalajs-ui. Every element
//! gets a wrapper box of its own, which is what lets an insert or a removal
//! touch one child instead of rebuilding the list. The index handed to the
//! body is the element's position at the time it is composed; like the
//! scalajs-ui default, later structural changes do not re-index siblings.

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use gtk::prelude::*;

use super::{
    components::{mount, Ctx},
    list::{ListChange, ListProperty},
    scope::{self, Scope},
    state::{CompositeDisposable, Disposable},
};

/// Composes `body` for every element, vertically and without spacing.
pub fn foreach<T: Clone + 'static>(
    list: &ListProperty<T>,
    body: impl Fn(&T, usize) + 'static,
) -> Ctx<gtk::Box> {
    foreach_in(gtk::Orientation::Vertical, 0, list, body)
}

/// Composes `body` for every element along `orientation`, `spacing` apart.
pub fn foreach_in<T: Clone + 'static>(
    orientation: gtk::Orientation,
    spacing: i32,
    list: &ListProperty<T>,
    body: impl Fn(&T, usize) + 'static,
) -> Ctx<gtk::Box> {
    let anchor = mount(gtk::Box::new(orientation, spacing), |_| {});
    let state = Rc::new(ForeachState {
        anchor: anchor.widget().clone(),
        parent_scope: Rc::downgrade(anchor.scope()),
        body: Box::new(body),
        items: RefCell::new(Vec::new()),
    });

    for (index, element) in list.get().iter().enumerate() {
        state.mount_at(index, element);
    }

    let subscription = {
        let state = Rc::clone(&state);
        list.observe_changes(move |change| state.apply(change))
    };
    anchor.add_disposable(subscription);
    anchor
}

type ItemBody<T> = Box<dyn Fn(&T, usize)>;

struct Item {
    host: gtk::Box,
    scope: Scope,
    release: Disposable,
}

struct ForeachState<T> {
    anchor: gtk::Box,
    parent_scope: Weak<CompositeDisposable>,
    body: ItemBody<T>,
    items: RefCell<Vec<Item>>,
}

impl<T: Clone + 'static> ForeachState<T> {
    fn apply(&self, change: &ListChange<T>) {
        match change {
            ListChange::Insert { index, element } => self.mount_at(*index, element),
            ListChange::Remove { index, .. } => self.unmount_at(*index),
            ListChange::Update { index, new, .. } => {
                self.unmount_at(*index);
                self.mount_at(*index, new);
            }
            ListChange::Reset { items } => {
                self.clear();
                for (index, element) in items.iter().enumerate() {
                    self.mount_at(index, element);
                }
            }
            ListChange::Clear => self.clear(),
        }
    }

    fn mount_at(&self, index: usize, element: &T) {
        let host = gtk::Box::new(self.anchor.orientation(), 0);
        let (index, sibling) = {
            let items = self.items.borrow();
            let index = index.min(items.len());
            let sibling = index
                .checked_sub(1)
                .map(|previous| items[previous].host.clone());
            (index, sibling)
        };
        self.anchor.insert_child_after(&host, sibling.as_ref());

        let scope: Scope = Rc::new(CompositeDisposable::new());
        let release = {
            let scope = Rc::clone(&scope);
            Disposable::new(move || scope.dispose())
        };
        if let Some(parent) = self.parent_scope.upgrade() {
            parent.add(release.clone());
        }
        scope::compose_into(Rc::new(host.clone()), &scope, || {
            (self.body)(element, index)
        });
        self.items.borrow_mut().insert(
            index,
            Item {
                host,
                scope,
                release,
            },
        );
    }

    fn unmount_at(&self, index: usize) {
        let item = {
            let mut items = self.items.borrow_mut();
            (index < items.len()).then(|| items.remove(index))
        };
        if let Some(item) = item {
            self.release(item);
        }
    }

    fn clear(&self) {
        let items = std::mem::take(&mut *self.items.borrow_mut());
        for item in items {
            self.release(item);
        }
    }

    fn release(&self, item: Item) {
        if let Some(parent) = self.parent_scope.upgrade() {
            parent.remove(&item.release);
        }
        item.scope.dispose();
        self.anchor.remove(&item.host);
    }
}
