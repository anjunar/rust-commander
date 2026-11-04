//! `popup_menu`: a context menu at a point, composed like any other block.
//!
//! The popover owns its scope: it is unparented when it closes, which ends
//! the scope and releases its handlers. Nothing has to remember the open menu
//! to close it later.

use gtk::{gdk, glib, prelude::*};

use super::{
    components::{button, Ctx},
    controls::separator,
    scope::render,
    state::IntoText,
};

/// The open menu, handed to the body of [`popup_menu`].
pub struct Menu {
    popover: gtk::Popover,
}

/// Opens a menu on `anchor` at the point `(x, y)` in the anchor's coordinates.
pub fn popup_menu(
    anchor: &impl IsA<gtk::Widget>,
    x: f64,
    y: f64,
    body: impl FnOnce(&Menu),
) -> gtk::Popover {
    let popover = gtk::Popover::new();
    popover.set_parent(anchor);
    popover.set_has_arrow(false);
    popover.set_position(gtk::PositionType::Bottom);
    popover.set_pointing_to(Some(&gdk::Rectangle::new(
        x.round() as i32,
        y.round() as i32,
        1,
        1,
    )));
    popover.add_css_class("menu");

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.set_margin_top(6);
    content.set_margin_bottom(6);
    content.set_margin_start(6);
    content.set_margin_end(6);
    popover.set_child(Some(&content));

    let menu = Menu {
        popover: popover.clone(),
    };
    render(&content, || body(&menu));

    // Unparenting inside `closed` would tear the popover down while GTK is
    // still delivering the signal; the next main-loop turn is safe.
    popover.connect_closed(|popover| {
        let popover = popover.clone();
        glib::idle_add_local_once(move || {
            if popover.parent().is_some() {
                popover.unparent();
            }
        });
    });
    popover.popup();
    popover
}

impl Menu {
    /// An entry that closes the menu and then runs `run`.
    pub fn item(&self, label: impl IntoText, run: impl Fn() + 'static) -> Ctx<gtk::Button> {
        let popover = self.popover.clone();
        button(label, |item| {
            item.css("flat");
            item.css("menu-item");
            item.set_halign(gtk::Align::Fill);
            if let Some(label) = item.child().and_downcast::<gtk::Label>() {
                label.set_xalign(0.0);
            }
            item.on_click(move || {
                popover.popdown();
                run();
            });
        })
    }

    /// An entry for an action that destroys data.
    pub fn destructive(&self, label: impl IntoText, run: impl Fn() + 'static) -> Ctx<gtk::Button> {
        let item = self.item(label, run);
        item.css("destructive-action");
        item
    }

    pub fn separator(&self) {
        separator(gtk::Orientation::Horizontal);
    }

    pub fn popover(&self) -> &gtk::Popover {
        &self.popover
    }
}
