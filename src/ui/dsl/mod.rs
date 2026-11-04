//! A composition DSL for GTK4, modelled on scalajs-ui.
//!
//! Elements, bindings and events are plain Rust calls; nesting follows the
//! widget tree; reactive state drives the widgets directly, and every
//! subscription ends with the widgets it belongs to. The counter from the
//! scalajs-ui README reads like this:
//!
//! ```ignore
//! use crate::ui::dsl::{button, render, text, vbox, Property};
//!
//! let count = Property::new(0);
//! render(&content, || {
//!     vbox(8, |_| {
//!         text(count.map(|value| format!("Count: {value}")), |_| {});
//!         button("Increment", |b| {
//!             let count = count.clone();
//!             b.on_click(move || count.update(|value| value + 1));
//!         });
//!     });
//! });
//! ```
//!
//! | scalajs-ui                              | here                                                |
//! | --------------------------------------- | --------------------------------------------------- |
//! | `render(this, cursor) { ... }`          | [`render`]`(&host, || { ... })`                     |
//! | `renderInto(target) { ... }`            | [`Ctx::compose`], [`adopt`]                         |
//! | `using AbstractComponent, Cursor`       | thread-local parent stack                           |
//! | `vbox { ... }`, `hbox { ... }`          | [`vbox`], [`hbox`], [`frame`], [`scrolled`]         |
//! | `text(value) {}`                        | [`text`], [`text_view`]                             |
//! | `button("Save") { onClick(...) }`       | [`button`]`("Save", |b| b.on_click(..))`            |
//! | `image`                                 | [`icon`], [`icon_button`]                           |
//! | `input`, `checkboxInput`, `comboBox`    | [`entry`], [`password`], [`check`], [`switch`], [`dropdown`], [`dropdown_list`], [`spin`] |
//! | `Condition.when(prop) { ... }`          | [`when`]                                            |
//! | `Foreach(list) { ... }`                 | [`foreach`], [`foreach_in`]                         |
//! | `Table`, `DataGrid`                     | [`column_view`], [`Table::column`], [`sorter_by`]   |
//! | `Property`, `ReadOnlyProperty`          | [`Property`], [`ReadOnly`], [`ReadOnly::combine`], [`from_notify`] |
//! | `ListProperty`                          | [`ListProperty`], [`ListChange`]                    |
//! | `Disposable`, `CompositeDisposable`     | [`Disposable`], [`CompositeDisposable`]             |
//! | i18n message state                      | [`tr!`], [`localized`], [`localized_with`], [`relocalize`] |
//! | viewport windows, overlays              | [`overlay`], [`paned`], [`header_bar`], [`Ctx::titlebar`], [`Ctx::popover`], [`popup_menu`] |
//! | `on("keydown")`, `onWindowKeyDown`      | [`Ctx::on_key_pressed`], [`Ctx::on_pressed`], [`Ctx::on_scroll`], [`Ctx::on_focus_change`], [`Ctx::on_close_request`] |
//! | commands                                | [`Ctx::action`] (`win.<name>` with accelerators)    |
//! | lifecycle-bound timers                  | [`Ctx::every`], [`Ctx::after`]                      |
//! | `DslLayer.child(component) { ... }`     | [`mount`], [`container`]                            |
//!
//! Feedback widgets have element functions too: [`spinner`], [`progress`],
//! [`separator`], [`scrollbar`]. Widgets the DSL has no element function for
//! are mounted with [`mount`]; a handle's [`Ctx::bind`] covers any
//! property-to-widget binding.

// The DSL is a library surface: elements and bindings exist for the dialogs
// that will use them, not only for the ones already migrated.
#![allow(dead_code)]

mod components;
mod controls;
mod foreach;
mod i18n;
mod list;
mod menu;
mod scope;
mod state;
mod table;
mod window;

#[allow(unused_imports)]
pub use components::{
    adopt, button, container, dropdown, entry, hbox, mount, scrolled, spin, switch, text, vbox,
    when, Ctx,
};
#[allow(unused_imports)]
pub use controls::{
    check, dropdown_list, frame, header_bar, icon, icon_button, overlay, paned, password, progress,
    scrollbar, separator, spinner, text_view,
};
#[allow(unused_imports)]
pub use foreach::{foreach, foreach_in};
#[allow(unused_imports)]
pub(crate) use i18n::tr;
#[allow(unused_imports)]
pub use i18n::{localized, localized_with, relocalize};
#[allow(unused_imports)]
pub use list::{ListChange, ListProperty};
#[allow(unused_imports)]
pub use menu::{popup_menu, Menu};
#[allow(unused_imports)]
pub use scope::{render, Host, Scope};
#[allow(unused_imports)]
pub use state::{CompositeDisposable, Disposable, IntoReadOnly, IntoText, Property, ReadOnly};
#[allow(unused_imports)]
pub use table::{column_view, sorter_by, Table};
#[allow(unused_imports)]
pub use window::from_notify;
