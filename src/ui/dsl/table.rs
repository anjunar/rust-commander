//! `column_view`: a typed table over a GTK list model.
//!
//! The rows stay in the model (`gio::ListStore` behind a selection), so GTK
//! keeps virtualising them; the DSL declares the columns. Each column gets a
//! `setup` body composed with the ordinary elements once per recycled cell,
//! and a `bind` function that pushes one row into those widgets.
//!
//! ```ignore
//! let table = column_view(&selection, |table| {
//!     table.column(
//!         tr!("column.name"),
//!         || text("", |_| {}).widget().clone(),
//!         |label: &gtk::Label, row: &FileRowObject| label.set_label(&row.name()),
//!     );
//!     table.on_row_activated(|position| open(position));
//! });
//! ```

use std::{cell::RefCell, collections::HashMap, ops::Deref, rc::Rc};

use gtk::{glib, prelude::*};

use super::{
    components::{mount, Ctx},
    scope::{self, Host, Scope},
    state::{CompositeDisposable, Disposable, IntoText},
};

type RowIndex = Rc<RefCell<HashMap<usize, u32>>>;

/// Binds a column's cells again: those of one row, or all of them.
type Refresher = Rc<dyn Fn(Option<u32>)>;

/// A mounted column view plus the bookkeeping that maps widgets to rows.
#[derive(Clone)]
pub struct Table {
    ctx: Ctx<gtk::ColumnView>,
    rows: RowIndex,
    refreshers: Rc<RefCell<Vec<Refresher>>>,
}

impl Deref for Table {
    type Target = Ctx<gtk::ColumnView>;

    fn deref(&self) -> &Ctx<gtk::ColumnView> {
        &self.ctx
    }
}

/// Mounts a column view over `model` and declares its columns in `body`.
pub fn column_view(model: &impl IsA<gtk::SelectionModel>, body: impl FnOnce(&Table)) -> Table {
    let ctx = mount(gtk::ColumnView::new(Some(model.clone())), |_| {});
    let table = Table {
        ctx,
        rows: Rc::new(RefCell::new(HashMap::new())),
        refreshers: Rc::new(RefCell::new(Vec::new())),
    };
    body(&table);
    table
}

struct ListItemSlot(gtk::ListItem);

impl Host for ListItemSlot {
    fn attach(&self, child: &gtk::Widget) {
        self.0.set_child(Some(child));
    }
}

struct CellState<C> {
    item: gtk::ListItem,
    handles: C,
    scope: Scope,
}

type Cells<C> = Rc<RefCell<HashMap<usize, CellState<C>>>>;

fn key(object: &impl IsA<glib::Object>) -> usize {
    object.upcast_ref::<glib::Object>().as_ptr() as usize
}

impl Table {
    /// Appends a column. `setup` composes one cell and returns the widgets
    /// `bind` needs; `bind` fills them from the row object of type `T`.
    pub fn column<T, C>(
        &self,
        title: impl IntoText,
        setup: impl Fn() -> C + 'static,
        bind: impl Fn(&C, &T) + 'static,
    ) -> gtk::ColumnViewColumn
    where
        T: IsA<glib::Object>,
        C: 'static,
    {
        self.column_with(
            title,
            setup,
            move |cell, row, _position| bind(cell, row),
            |_, _| {},
        )
    }

    /// Like [`column`](Self::column), for cells that care about their place:
    /// `bind` also gets the row's position, and `unbind` runs when a cell
    /// stops showing a row, before GTK recycles it for another one.
    pub fn column_with<T, C>(
        &self,
        title: impl IntoText,
        setup: impl Fn() -> C + 'static,
        bind: impl Fn(&C, &T, u32) + 'static,
        unbind: impl Fn(&C, &T) + 'static,
    ) -> gtk::ColumnViewColumn
    where
        T: IsA<glib::Object>,
        C: 'static,
    {
        let bind = Rc::new(bind);
        let factory = gtk::SignalListItemFactory::new();
        let cells: Cells<C> = Rc::new(RefCell::new(HashMap::new()));

        {
            let cells = Rc::clone(&cells);
            factory.connect_setup(move |_, object| {
                let Some(item) = object.downcast_ref::<gtk::ListItem>() else {
                    return;
                };
                let scope: Scope = Rc::new(CompositeDisposable::new());
                let handles =
                    scope::compose_into(Rc::new(ListItemSlot(item.clone())), &scope, &setup);
                cells.borrow_mut().insert(
                    key(item),
                    CellState {
                        item: item.clone(),
                        handles,
                        scope,
                    },
                );
            });
        }
        {
            let cells = Rc::clone(&cells);
            let rows = Rc::clone(&self.rows);
            let bind = Rc::clone(&bind);
            factory.connect_bind(move |_, object| {
                let Some(item) = object.downcast_ref::<gtk::ListItem>() else {
                    return;
                };
                let Some(row) = item.item().and_then(|row| row.downcast::<T>().ok()) else {
                    return;
                };
                if let Some(cell) = cells.borrow().get(&key(item)) {
                    bind(&cell.handles, &row, item.position());
                }
                if let Some(child) = item.child() {
                    rows.borrow_mut().insert(key(&child), item.position());
                }
            });
        }
        {
            let cells = Rc::clone(&cells);
            let rows = Rc::clone(&self.rows);
            factory.connect_unbind(move |_, object| {
                let Some(item) = object.downcast_ref::<gtk::ListItem>() else {
                    return;
                };
                if let Some(row) = item.item().and_then(|row| row.downcast::<T>().ok()) {
                    if let Some(cell) = cells.borrow().get(&key(item)) {
                        unbind(&cell.handles, &row);
                    }
                }
                if let Some(child) = item.child() {
                    rows.borrow_mut().remove(&key(&child));
                }
            });
        }
        {
            let cells = Rc::clone(&cells);
            factory.connect_teardown(move |_, object| {
                let Some(item) = object.downcast_ref::<gtk::ListItem>() else {
                    return;
                };
                let cell = cells.borrow_mut().remove(&key(item));
                if let Some(cell) = cell {
                    cell.scope.dispose();
                }
            });
        }
        {
            let cells = Rc::clone(&cells);
            self.refreshers
                .borrow_mut()
                .push(Rc::new(move |position: Option<u32>| {
                    for cell in cells.borrow().values() {
                        let current = cell.item.position();
                        if position.is_some_and(|position| position != current) {
                            continue;
                        }
                        let row = cell.item.item().and_then(|row| row.downcast::<T>().ok());
                        if let Some(row) = row {
                            bind(&cell.handles, &row, current);
                        }
                    }
                }));
        }
        self.add_disposable(Disposable::new(move || {
            let released = std::mem::take(&mut *cells.borrow_mut());
            for cell in released.into_values() {
                cell.scope.dispose();
            }
        }));

        let column = gtk::ColumnViewColumn::new(None, Some(factory));
        {
            let column = column.clone();
            self.add_disposable(
                title
                    .into_text()
                    .observe(move |title| column.set_title(Some(title.as_str()))),
            );
        }
        self.widget().append_column(&column);
        column
    }

    /// Binds the cells of one row again, after its object changed in place.
    ///
    /// The list model stays untouched, so the selection and the scroll
    /// position survive. Rows GTK has no cell for are left alone: they are
    /// bound when they scroll into view.
    pub fn refresh_row(&self, position: u32) {
        for refresh in self.refreshers.borrow().iter() {
            refresh(Some(position));
        }
    }

    /// Binds every cell on screen again.
    pub fn refresh_rows(&self) {
        for refresh in self.refreshers.borrow().iter() {
            refresh(None);
        }
    }

    /// The row under a point given in the view's coordinates.
    pub fn row_at(&self, x: f64, y: f64) -> Option<u32> {
        let view = self.widget().upcast_ref::<gtk::Widget>().clone();
        let rows = self.rows.borrow();
        let mut current = view.pick(x, y, gtk::PickFlags::DEFAULT);
        while let Some(widget) = current {
            if let Some(position) = rows.get(&key(&widget)) {
                return Some(*position);
            }
            if widget == view {
                break;
            }
            current = widget.parent();
        }
        None
    }

    /// Runs when a row is activated (double click or Enter, depending on the view).
    pub fn on_row_activated(&self, handler: impl Fn(u32) + 'static) {
        let handler = self
            .widget()
            .connect_activate(move |_, position| handler(position));
        self.own_signal(handler);
    }

    /// Runs when the user sorts by a column header.
    pub fn on_sort_changed(
        &self,
        handler: impl Fn(&gtk::ColumnViewColumn, gtk::SortType) + 'static,
    ) {
        let Some(sorter) = self
            .widget()
            .sorter()
            .and_then(|sorter| sorter.downcast::<gtk::ColumnViewSorter>().ok())
        else {
            return;
        };
        let handler = sorter.connect_changed(move |sorter, _| {
            if let Some(column) = sorter.primary_sort_column() {
                handler(&column, sorter.primary_sort_order());
            }
        });
        self.own_object_signal(&sorter, handler);
    }
}

/// A sorter for a column, comparing two row objects of type `T`.
pub fn sorter_by<T: IsA<glib::Object>>(
    compare: impl Fn(&T, &T) -> std::cmp::Ordering + 'static,
) -> gtk::CustomSorter {
    gtk::CustomSorter::new(move |first, second| {
        let (Some(first), Some(second)) = (first.downcast_ref::<T>(), second.downcast_ref::<T>())
        else {
            return gtk::Ordering::Equal;
        };
        compare(first, second).into()
    })
}
