use std::rc::Rc;

use gtk::prelude::*;
use rust_i18n::t;
use sourceview5::prelude::*;

use super::EditorWidgets;

pub(super) fn show_find_bar(widgets: &EditorWidgets, with_replace: bool) {
    widgets.replace_row.set_visible(with_replace);
    widgets.find_revealer.set_reveal_child(true);
    if let Some((start, end)) = widgets.buffer.selection_bounds() {
        if start.line() == end.line() && start.offset() != end.offset() {
            let selected = widgets.buffer.text(&start, &end, false);
            if !selected.is_empty() {
                widgets.find_entry.set_text(&selected);
            }
        }
    }
    widgets.find_entry.grab_focus();
    widgets.find_entry.select_region(0, -1);
}

pub(super) fn hide_find_bar(widgets: &EditorWidgets) {
    widgets.find_revealer.set_reveal_child(false);
    widgets.replace_row.set_visible(false);
    widgets.search_settings.set_search_text(None::<&str>);
}

fn sync_search_text(widgets: &EditorWidgets) {
    let text = widgets.find_entry.text();
    if text.is_empty() {
        widgets.search_settings.set_search_text(None::<&str>);
    } else {
        widgets.search_settings.set_search_text(Some(text.as_str()));
    }
}

pub(super) fn find_next(widgets: &EditorWidgets) {
    if !widgets.find_revealer.reveals_child() {
        show_find_bar(widgets, widgets.replace_row.is_visible());
        return;
    }
    sync_search_text(widgets);
    if widgets.find_entry.text().is_empty() {
        return;
    }
    let mut iter = widgets.buffer.iter_at_mark(&widgets.buffer.get_insert());
    if widgets.buffer.selection_bounds().is_some() {
        iter.forward_char();
    }
    if let Some((start, end, _)) = widgets.search_context.forward(&iter) {
        widgets.buffer.select_range(&start, &end);
        let mut start = start;
        let _ = widgets
            .view
            .scroll_to_iter(&mut start, 0.1, false, 0.0, 0.0);
        let _ = end;
    }
}

pub(super) fn find_previous(widgets: &EditorWidgets) {
    if !widgets.find_revealer.reveals_child() {
        show_find_bar(widgets, widgets.replace_row.is_visible());
        return;
    }
    sync_search_text(widgets);
    if widgets.find_entry.text().is_empty() {
        return;
    }
    let iter = widgets.buffer.iter_at_mark(&widgets.buffer.get_insert());
    if let Some((start, end, _)) = widgets.search_context.backward(&iter) {
        widgets.buffer.select_range(&start, &end);
        let mut start = start;
        let _ = widgets
            .view
            .scroll_to_iter(&mut start, 0.1, false, 0.0, 0.0);
        let _ = end;
    }
}

pub(super) fn replace_current_match(widgets: &EditorWidgets) {
    sync_search_text(widgets);
    let replace = widgets.replace_entry.text();
    if let Some((mut start, mut end)) = widgets.buffer.selection_bounds() {
        if widgets
            .search_context
            .replace(&mut start, &mut end, replace.as_str())
            .is_ok()
        {
            find_next(widgets);
            return;
        }
    }
    find_next(widgets);
}

pub(super) fn go_to_line(widgets: &Rc<EditorWidgets>) {
    let dialog = gtk::Window::builder()
        .transient_for(&widgets.window)
        .modal(true)
        .title(t!("editor.go_to_line_title").into_owned())
        .default_width(320)
        .build();
    if let Some(app) = widgets.window.application() {
        dialog.set_application(Some(&app));
    }

    let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);

    let label = gtk::Label::new(Some(&t!("editor.go_to_line_prompt")));
    label.set_xalign(0.0);
    root.append(&label);

    let entry = gtk::Entry::new();
    entry.set_input_purpose(gtk::InputPurpose::Digits);
    root.append(&entry);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label(&t!("common.cancel"));
    let ok = gtk::Button::with_label(&t!("common.ok"));
    ok.add_css_class("suggested-action");
    buttons.append(&cancel);
    buttons.append(&ok);
    root.append(&buttons);
    dialog.set_child(Some(&root));
    dialog.set_default_widget(Some(&ok));

    // The handlers live inside the dialog, so they hold it and the entry weakly.
    // A strong reference would keep the dialog, and through it the editor, alive.
    let close_dialog = {
        let dialog = dialog.downgrade();
        move || {
            if let Some(dialog) = dialog.upgrade() {
                dialog.close();
            }
        }
    };

    let apply_goto = Rc::new({
        let widgets = Rc::clone(widgets);
        let entry = entry.downgrade();
        let close_dialog = close_dialog.clone();
        move || {
            let Some(entry) = entry.upgrade() else {
                return;
            };
            let Ok(line) = entry.text().parse::<i32>() else {
                return;
            };
            let total_lines = widgets.buffer.line_count().max(1);
            let target = line.clamp(1, total_lines) - 1;
            if let Some(mut iter) = widgets.buffer.iter_at_line(target) {
                widgets.buffer.place_cursor(&iter);
                let _ = widgets.view.scroll_to_iter(&mut iter, 0.2, true, 0.0, 0.0);
                widgets.view.grab_focus();
            }
            close_dialog();
        }
    });

    cancel.connect_clicked(move |_| close_dialog());
    {
        let apply_goto = Rc::clone(&apply_goto);
        ok.connect_clicked(move |_| apply_goto());
    }
    {
        entry.connect_activate(move |_| apply_goto());
    }

    dialog.present();
    entry.grab_focus();
}
