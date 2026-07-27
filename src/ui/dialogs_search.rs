use std::{
    cell::{Cell, RefCell},
    ffi::OsString,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use gtk::{gio, glib, prelude::*};
use rust_i18n::t;

use crate::{
    domain::{
        entry_key::EntryKey,
        search::SearchOptions,
        selection::{SelectionIntent, SelectionSnapshot},
    },
    fs::search::{spawn_search, SearchEvent},
    ui::glib_poll,
};

use super::dialogs_base::{build_modal_window, present_with_focus, ModalWindow};

/// Action emitted when the user activates a search result.
pub enum SearchDialogAction {
    GoTo {
        directory: PathBuf,
        focus_name: String,
    },
}

pub fn show_find_files<F>(
    parent: &gtk::ApplicationWindow,
    initial_root: Option<PathBuf>,
    show_hidden_files: bool,
    on_action: F,
) where
    F: Fn(SearchDialogAction) + 'static,
{
    let ModalWindow {
        window,
        content,
        actions,
        ..
    } = build_modal_window(parent, &t!("search.title"), 680, 560);

    let description = gtk::Label::new(Some(&t!("search.description")));
    description.set_xalign(0.0);
    description.set_wrap(true);
    content.append(&description);

    let local_root = initial_root.filter(|path| path.is_dir());
    let search_allowed = local_root.is_some();

    if !search_allowed {
        let notice = gtk::Label::new(Some(&t!("search.not_local")));
        notice.set_xalign(0.0);
        notice.set_wrap(true);
        notice.add_css_class("dim-label");
        content.append(&notice);
    }

    let name_row = labeled_entry_row(&t!("search.name_label"), &t!("search.name_placeholder"));
    let name_entry = name_row.1;
    content.append(&name_row.0);

    let exclude_row = labeled_entry_row(
        &t!("search.exclude_label"),
        &t!("search.exclude_placeholder"),
    );
    let exclude_entry = exclude_row.1;
    exclude_entry.set_sensitive(search_allowed);
    content.append(&exclude_row.0);

    let location_row = labeled_entry_row(&t!("search.location_label"), "");
    let location_entry = location_row.1;
    if let Some(root) = &local_root {
        location_entry.set_text(&root.display().to_string());
    }
    location_entry.set_sensitive(search_allowed);
    content.append(&location_row.0);

    let options_row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    let include_subfolders = gtk::CheckButton::with_label(&t!("search.include_subfolders"));
    include_subfolders.set_active(true);
    include_subfolders.set_sensitive(search_allowed);
    let case_sensitive = gtk::CheckButton::with_label(&t!("search.case_sensitive"));
    case_sensitive.set_active(false);
    case_sensitive.set_sensitive(search_allowed);
    let use_regex = gtk::CheckButton::with_label(&t!("search.use_regex"));
    use_regex.set_active(false);
    use_regex.set_sensitive(search_allowed);
    options_row.append(&include_subfolders);
    options_row.append(&case_sensitive);
    options_row.append(&use_regex);
    content.append(&options_row);

    let store = gio::ListStore::new::<SearchHitObject>();
    let selection = gtk::SingleSelection::new(Some(store.clone()));
    selection.set_autoselect(true);
    selection.set_can_unselect(false);

    let column_view = gtk::ColumnView::new(Some(selection.clone()));
    column_view.set_reorderable(false);
    column_view.set_hexpand(true);
    column_view.set_vexpand(true);
    column_view.add_css_class("data-table");
    column_view.add_css_class("search-results");

    append_text_column(&column_view, &t!("column.name"), 160, true, |hit| {
        hit.name()
    });
    append_text_column(&column_view, &t!("column.type"), 70, false, |hit| {
        hit.type_label()
    });
    append_text_column(&column_view, &t!("search.column_path"), 240, true, |hit| {
        hit.parent_display()
    });
    append_text_column(&column_view, &t!("column.size"), 90, false, |hit| {
        hit.size_label()
    });
    append_text_column(&column_view, &t!("column.modified"), 130, false, |hit| {
        hit.modified_label()
    });

    let scrolled = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&column_view)
        .build();
    scrolled.add_css_class("panel-scroller");
    content.append(&scrolled);

    let status_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let spinner = gtk::Spinner::new();
    let status_label = gtk::Label::new(Some(&t!("search.status_idle")));
    status_label.set_xalign(0.0);
    status_label.set_hexpand(true);
    status_label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    status_label.add_css_class("dim-label");
    status_row.append(&spinner);
    status_row.append(&status_label);
    content.append(&status_row);

    let start_button = gtk::Button::with_label(&t!("search.start"));
    start_button.add_css_class("suggested-action");
    start_button.set_sensitive(search_allowed);
    let stop_button = gtk::Button::with_label(&t!("search.stop"));
    stop_button.set_sensitive(false);
    let close_button = gtk::Button::with_label(&t!("common.close"));
    actions.append(&start_button);
    actions.append(&stop_button);
    actions.append(&close_button);

    let callback: Rc<dyn Fn(SearchDialogAction)> = Rc::new(on_action);
    let cancel_flag = Rc::new(RefCell::new(None::<Arc<AtomicBool>>));
    let search_generation = Rc::new(Cell::new(0u64));
    let found_count = Rc::new(Cell::new(0usize));
    let searching = Rc::new(Cell::new(false));

    let set_searching = {
        let spinner = spinner.clone();
        let start_button = start_button.clone();
        let stop_button = stop_button.clone();
        let name_entry = name_entry.clone();
        let exclude_entry = exclude_entry.clone();
        let location_entry = location_entry.clone();
        let include_subfolders = include_subfolders.clone();
        let case_sensitive = case_sensitive.clone();
        let use_regex = use_regex.clone();
        let searching = Rc::clone(&searching);
        let search_allowed = search_allowed;
        Rc::new(move |active: bool| {
            searching.set(active);
            if active {
                spinner.start();
            } else {
                spinner.stop();
            }
            spinner.set_visible(active);
            stop_button.set_sensitive(active);
            start_button.set_sensitive(search_allowed && !active);
            name_entry.set_sensitive(search_allowed && !active);
            exclude_entry.set_sensitive(search_allowed && !active);
            location_entry.set_sensitive(search_allowed && !active);
            include_subfolders.set_sensitive(search_allowed && !active);
            case_sensitive.set_sensitive(search_allowed && !active);
            use_regex.set_sensitive(search_allowed && !active);
        })
    };
    set_searching(false);
    spinner.set_visible(false);

    let go_selected = {
        let selection = selection.clone();
        let callback = Rc::clone(&callback);
        Rc::new(move || {
            let Some(item) = selection.selected_item() else {
                return;
            };
            let Ok(hit) = item.downcast::<SearchHitObject>() else {
                return;
            };
            let path = hit.path();
            let Some((directory, focus_name)) = split_go_to(&path) else {
                return;
            };
            callback(SearchDialogAction::GoTo {
                directory,
                focus_name,
            });
        })
    };

    {
        let go_selected = Rc::clone(&go_selected);
        column_view.connect_activate(move |_, _| {
            go_selected();
        });
    }

    let stop_search = {
        let cancel_flag = Rc::clone(&cancel_flag);
        let set_searching = Rc::clone(&set_searching);
        let status_label = status_label.clone();
        let found_count = Rc::clone(&found_count);
        Rc::new(move || {
            if let Some(flag) = cancel_flag.borrow_mut().take() {
                flag.store(true, Ordering::Relaxed);
            }
            set_searching(false);
            status_label.set_text(&t!("search.status_cancelled", count = found_count.get()));
        })
    };

    let start_search = {
        let window = window.clone();
        let store = store.clone();
        let name_entry = name_entry.clone();
        let exclude_entry = exclude_entry.clone();
        let location_entry = location_entry.clone();
        let include_subfolders = include_subfolders.clone();
        let case_sensitive = case_sensitive.clone();
        let use_regex = use_regex.clone();
        let status_label = status_label.clone();
        let cancel_flag = Rc::clone(&cancel_flag);
        let search_generation = Rc::clone(&search_generation);
        let found_count = Rc::clone(&found_count);
        let set_searching = Rc::clone(&set_searching);
        let searching = Rc::clone(&searching);
        let show_hidden_files = show_hidden_files;

        Rc::new(move || {
            if !search_allowed || searching.get() {
                return;
            }

            let root_text = location_entry.text().to_string();
            let root = PathBuf::from(root_text.trim());
            if root_text.trim().is_empty() || !root.is_dir() {
                status_label.set_text(&t!("search.invalid_location"));
                return;
            }

            if let Some(flag) = cancel_flag.borrow_mut().take() {
                flag.store(true, Ordering::Relaxed);
            }

            store.remove_all();
            found_count.set(0);

            let generation = search_generation.get().wrapping_add(1);
            search_generation.set(generation);

            let flag = Arc::new(AtomicBool::new(false));
            *cancel_flag.borrow_mut() = Some(Arc::clone(&flag));

            let options = SearchOptions {
                root,
                name_mask: name_entry.text().to_string(),
                exclude_mask: exclude_entry.text().to_string(),
                include_subfolders: include_subfolders.is_active(),
                case_sensitive: case_sensitive.is_active(),
                use_regex: use_regex.is_active(),
                show_hidden_files,
            };

            if let Err(error) = crate::domain::search::compile_search(&options) {
                *cancel_flag.borrow_mut() = None;
                status_label.set_text(&t!("search.invalid_pattern", error = error));
                return;
            }

            set_searching(true);
            status_label.set_text(&t!(
                "search.status_searching",
                path = options.root.display().to_string(),
                count = 0
            ));

            let rx = spawn_search(options, flag);
            let store = store.clone();
            let status_label = status_label.clone();
            let found_count = Rc::clone(&found_count);
            let set_searching = Rc::clone(&set_searching);
            let cancel_flag = Rc::clone(&cancel_flag);
            let search_generation = Rc::clone(&search_generation);
            let window_alive = window.clone();

            glib_poll::poll_receiver(
                Duration::from_millis(30),
                rx,
                move |event| {
                    if !window_alive.is_visible() {
                        return false;
                    }
                    if search_generation.get() != generation {
                        return false;
                    }
                    match event {
                        SearchEvent::Progress { path, found } => {
                            found_count.set(found);
                            status_label.set_text(&t!(
                                "search.status_searching",
                                path = path.display().to_string(),
                                count = found
                            ));
                            true
                        }
                        SearchEvent::Hits(hits) => {
                            for hit in hits {
                                store.append(&SearchHitObject::new(hit));
                            }
                            true
                        }
                        SearchEvent::Failed { message } => {
                            *cancel_flag.borrow_mut() = None;
                            set_searching(false);
                            status_label.set_text(&t!("search.invalid_pattern", error = message));
                            false
                        }
                        SearchEvent::Finished {
                            found,
                            errors,
                            cancelled,
                        } => {
                            found_count.set(found);
                            *cancel_flag.borrow_mut() = None;
                            set_searching(false);
                            if cancelled {
                                status_label
                                    .set_text(&t!("search.status_cancelled", count = found));
                            } else if errors > 0 {
                                status_label.set_text(&t!(
                                    "search.status_done_with_errors",
                                    count = found,
                                    errors = errors
                                ));
                            } else {
                                status_label.set_text(&t!("search.status_done", count = found));
                            }
                            false
                        }
                    }
                },
                move || false,
            );
        })
    };

    {
        let start_search = Rc::clone(&start_search);
        start_button.connect_clicked(move |_| {
            start_search();
        });
    }
    {
        let start_search = Rc::clone(&start_search);
        name_entry.connect_activate(move |_| {
            start_search();
        });
    }
    {
        let start_search = Rc::clone(&start_search);
        exclude_entry.connect_activate(move |_| {
            start_search();
        });
    }
    {
        let start_search = Rc::clone(&start_search);
        location_entry.connect_activate(move |_| {
            start_search();
        });
    }
    {
        let stop_search = Rc::clone(&stop_search);
        stop_button.connect_clicked(move |_| {
            stop_search();
        });
    }
    {
        let window = window.clone();
        close_button.connect_clicked(move |_| {
            window.close();
        });
    }
    {
        let cancel_flag = Rc::clone(&cancel_flag);
        window.connect_close_request(move |_| {
            if let Some(flag) = cancel_flag.borrow_mut().take() {
                flag.store(true, Ordering::Relaxed);
            }
            glib::Propagation::Proceed
        });
    }

    present_with_focus(window, &name_entry);
}

fn labeled_entry_row(label_text: &str, placeholder: &str) -> (gtk::Box, gtk::Entry) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let label = gtk::Label::new(Some(label_text));
    label.set_xalign(0.0);
    label.set_width_chars(12);
    let entry = gtk::Entry::new();
    if !placeholder.is_empty() {
        entry.set_placeholder_text(Some(placeholder));
    }
    entry.set_hexpand(true);
    row.append(&label);
    row.append(&entry);
    (row, entry)
}

fn split_go_to(path: &Path) -> Option<(PathBuf, String)> {
    let focus_name = path.file_name()?.to_string_lossy().into_owned();
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(path));
    Some((directory, focus_name))
}

fn append_text_column<F>(
    column_view: &gtk::ColumnView,
    title: &str,
    width: i32,
    expand: bool,
    text: F,
) where
    F: Fn(&SearchHitObject) -> String + 'static,
{
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(move |_, item| {
        let Some(list_item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let label = gtk::Label::new(None);
        label.set_xalign(0.0);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        label.set_hexpand(true);
        list_item.set_child(Some(&label));
    });
    factory.connect_bind(move |_, item| {
        let Some(list_item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(hit) = list_item
            .item()
            .and_then(|obj| obj.downcast::<SearchHitObject>().ok())
        else {
            return;
        };
        if let Some(label) = list_item
            .child()
            .and_then(|child| child.downcast::<gtk::Label>().ok())
        {
            label.set_text(&text(&hit));
        }
    });

    let column = gtk::ColumnViewColumn::new(Some(title), Some(factory));
    column.set_resizable(true);
    column.set_expand(expand);
    column.set_fixed_width(width);
    column_view.append_column(&column);
}

mod search_hit_object {
    use std::{cell::RefCell, path::PathBuf};

    use gtk::glib::{self, subclass::prelude::*};
    use rust_i18n::t;

    use crate::{
        domain::search::SearchHit,
        fs::reader::{format_bytes, format_system_time},
    };

    #[derive(Clone, Debug, Default)]
    struct SearchHitData {
        path: PathBuf,
        name: String,
        type_label: String,
        parent_display: String,
        size_label: String,
        modified_label: String,
    }

    mod imp {
        use super::*;

        #[derive(Default)]
        pub struct SearchHitObject {
            pub(super) data: RefCell<SearchHitData>,
        }

        #[glib::object_subclass]
        impl ObjectSubclass for SearchHitObject {
            const NAME: &'static str = "RustCommanderSearchHitObject";
            type Type = super::SearchHitObject;
        }

        impl ObjectImpl for SearchHitObject {}
    }

    glib::wrapper! {
        pub struct SearchHitObject(ObjectSubclass<imp::SearchHitObject>);
    }

    impl SearchHitObject {
        pub fn new(hit: SearchHit) -> Self {
            let object: Self = glib::Object::new();
            let parent_display = hit
                .path
                .parent()
                .map(|parent| parent.display().to_string())
                .unwrap_or_default();
            let type_label = if hit.is_dir {
                t!("entry.folder").into_owned()
            } else {
                t!("entry.file").into_owned()
            };
            let size_label = if hit.is_dir {
                "-".into()
            } else {
                format_bytes(hit.size_bytes)
            };
            let modified_label = hit
                .modified_at
                .map(format_system_time)
                .unwrap_or_else(|| "-".into());
            *object.imp().data.borrow_mut() = SearchHitData {
                path: hit.path,
                name: hit.name,
                type_label,
                parent_display,
                size_label,
                modified_label,
            };
            object
        }

        pub fn path(&self) -> PathBuf {
            self.imp().data.borrow().path.clone()
        }

        pub fn name(&self) -> String {
            self.imp().data.borrow().name.clone()
        }

        pub fn type_label(&self) -> String {
            self.imp().data.borrow().type_label.clone()
        }

        pub fn parent_display(&self) -> String {
            self.imp().data.borrow().parent_display.clone()
        }

        pub fn size_label(&self) -> String {
            self.imp().data.borrow().size_label.clone()
        }

        pub fn modified_label(&self) -> String {
            self.imp().data.borrow().modified_label.clone()
        }
    }
}

use search_hit_object::SearchHitObject;

/// Build a navigation request that opens `directory` and focuses `focus_name`.
pub fn search_go_to_request(
    panel: crate::application::ActivePanel,
    directory: PathBuf,
    focus_name: String,
) -> crate::application::NavigationRequest {
    use crate::application::{filesystem_navigation_request, LoadAction};

    let mut request = filesystem_navigation_request(panel, directory.clone());
    request.action = LoadAction::Navigate;
    request.status = t!(
        "search.status_opened",
        path = directory.join(&focus_name).display().to_string()
    )
    .into_owned();
    request.selection_intent = Some(SelectionIntent::reveal(
        EntryKey::FilesystemName(OsString::from(focus_name)),
        SelectionSnapshot::default(),
    ));
    request
}
