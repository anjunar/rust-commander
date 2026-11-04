use std::{cell::RefCell, rc::Rc};

use gtk::{glib, prelude::*};
use rust_i18n::t;

use crate::ui::dsl::{
    adopt, button, entry, hbox, render, switch, text, vbox, Ctx, IntoText, Property,
};

/// A modal window under construction: the window itself plus the two areas a
/// dialog fills, its content and its row of action buttons.
pub(crate) struct Modal {
    window: Ctx<gtk::Window>,
    content: Ctx<gtk::Box>,
    actions: Ctx<gtk::Box>,
}

impl Modal {
    pub(crate) fn window(&self) -> &Ctx<gtk::Window> {
        &self.window
    }

    /// Composes the dialog's content.
    pub(crate) fn content<R>(&self, body: impl FnOnce() -> R) -> R {
        self.content.compose(body)
    }

    /// Composes the dialog's action buttons, right-aligned below the content.
    pub(crate) fn actions<R>(&self, body: impl FnOnce() -> R) -> R {
        self.actions.compose(body)
    }

    /// A button that closes the dialog.
    pub(crate) fn close_button(&self, label: impl IntoText) -> Ctx<gtk::Button> {
        let window = self.window.widget().clone();
        button(label, |button| button.on_click(move || window.close()))
    }
}

/// A modal window over `parent`, composed in one block.
///
/// Returns the window and whatever `body` returns, typically the widget that
/// should receive the default action or the initial focus. Closing the
/// window hands the focus back to `parent`.
pub(crate) fn modal<R>(
    parent: &gtk::ApplicationWindow,
    title: &str,
    (width, height): (i32, i32),
    body: impl FnOnce(&Modal) -> R,
) -> (gtk::Window, R) {
    let window = gtk::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title(title)
        .default_width(width)
        .default_height(height)
        .build();

    let result = render(&window, || {
        let root = adopt(&window);
        {
            let parent = parent.clone();
            root.on_close_request(move || {
                let parent = parent.clone();
                glib::idle_add_local_once(move || {
                    parent.present();
                    parent.grab_focus();
                });
                false
            });
        }

        #[cfg(target_os = "windows")]
        root.titlebar(|| dialog_titlebar(&window, title));

        let mut areas = None;
        vbox(10, |frame| {
            frame.set_margin_top(12);
            frame.set_margin_bottom(14);
            frame.set_margin_start(12);
            frame.set_margin_end(12);
            let content = vbox(10, |_| {});
            let actions = hbox(8, |actions| {
                actions.set_halign(gtk::Align::End);
                actions.set_margin_top(3);
            });
            areas = Some((content, actions));
        });
        let (content, actions) = areas.expect("dialog areas composed");

        body(&Modal {
            window: root,
            content,
            actions,
        })
    });

    (window, result)
}

/// The Windows titlebar of a dialog: the title and a close button, matching
/// the custom window controls of the main window.
#[cfg(target_os = "windows")]
fn dialog_titlebar(window: &gtk::Window, title: &str) -> Ctx<gtk::HeaderBar> {
    use crate::ui::dsl::{header_bar, icon_button};

    header_bar(|header| {
        header.set_show_title_buttons(false);
        header.title(|| text(title, |label| label.css("app-title")));
        header.pack_end(|| {
            hbox(4, |controls| {
                controls.css("window-controls");
                let window = window.clone();
                icon_button("window-close-symbolic", |close| {
                    close.css("window-control-button");
                    close.css("window-close-button");
                    close.css("flat");
                    close.set_focus_on_click(false);
                    close.set_size_request(44, 28);
                    close.set_tooltip_text(Some("Close"));
                    close.on_click(move || window.close());
                });
            })
        });
    })
}

pub fn show_error(parent: &gtk::ApplicationWindow, title: &str, detail: &str) {
    let (window, close) = modal(parent, title, (460, 180), |dialog| {
        dialog.content(|| {
            heading(title);
            paragraph(detail);
        });
        dialog.actions(|| {
            let close = dialog.close_button(t!("common.close"));
            close.css("suggested-action");
            close
        })
    });
    window.set_default_widget(Some(close.widget()));
    window.present();
}

// ---------------------------------------------------------------------------
// Building blocks shared by the dialogs
// ---------------------------------------------------------------------------

/// A bold heading that wraps.
pub(crate) fn heading(value: impl IntoText) -> Ctx<gtk::Label> {
    text(value, |label| {
        label.set_wrap(true);
        label.css("dialog-title");
    })
}

/// A body text that wraps.
pub(crate) fn paragraph(value: impl IntoText) -> Ctx<gtk::Label> {
    text(value, |label| label.set_wrap(true))
}

/// A dimmed, wrapping note.
pub(crate) fn hint(value: impl IntoText) -> Ctx<gtk::Label> {
    text(value, |label| {
        label.set_wrap(true);
        label.css("dim-label");
    })
}

/// A label on the left and the controls composed by `body` on the right.
pub(crate) fn row(label: impl IntoText, body: impl FnOnce()) -> Ctx<gtk::Box> {
    hbox(8, |_| {
        text(label, |label| label.set_hexpand(true));
        body();
    })
}

/// A row with a switch bound both ways to `value`.
pub(crate) fn switch_row(label: impl IntoText, value: &Property<bool>) -> Ctx<gtk::Box> {
    row(label, || {
        switch(value, |_| {});
    })
}

/// A full-width entry with a placeholder, bound both ways to `value`.
pub(crate) fn field(placeholder: &str, value: &Property<String>) -> Ctx<gtk::Entry> {
    entry(value, |entry| {
        entry.set_hexpand(true);
        entry.set_placeholder_text(Some(placeholder));
    })
}

/// Presents `window` on the next main-loop turn and focuses `focus`.
pub(crate) fn present_and_focus(window: &gtk::Window, focus: &impl IsA<gtk::Widget>) {
    let window = window.clone();
    let focus = focus.clone();
    glib::idle_add_local_once(move || {
        window.present();
        focus.grab_focus();
    });
}

/// A callback that runs at most once, however many handlers share it.
///
/// Dialog results are `FnOnce` callbacks, while GTK handlers are `Fn` and a
/// dialog has several of them; this is the one place that bridges the two.
pub(crate) struct Once<T>(OnceCallback<T>);

type OnceCallback<T> = Rc<RefCell<Option<Box<dyn FnOnce(T)>>>>;

impl<T> Clone for Once<T> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<T> Once<T> {
    pub(crate) fn new(callback: impl FnOnce(T) + 'static) -> Self {
        Self(Rc::new(RefCell::new(Some(Box::new(callback)))))
    }

    pub(crate) fn call(&self, value: T) {
        let callback = self.0.borrow_mut().take();
        if let Some(callback) = callback {
            callback(value);
        }
    }
}
