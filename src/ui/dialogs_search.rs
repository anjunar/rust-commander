use std::{
    cell::{Cell, RefCell},
    ffi::OsString,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, TryRecvError},
        Arc,
    },
    time::Duration,
};

use gtk::{gio, prelude::*};
use rust_i18n::t;

use crate::{
    domain::{
        entry_key::EntryKey,
        search::{compile_search, SearchOptions},
        selection::{SelectionIntent, SelectionSnapshot},
    },
    fs::search::{spawn_search, SearchEvent},
    ui::dsl::{
        button, check, column_view, hbox, scrolled, spinner, text, Ctx, IntoText, Property,
        ReadOnly, Table,
    },
};

use super::dialogs_base::{field, hint, modal, paragraph, present_and_focus};

const POLL_INTERVAL: Duration = Duration::from_millis(30);

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
    let form = Rc::new(SearchForm::new(
        initial_root.filter(|path| path.is_dir()),
        show_hidden_files,
    ));

    let selection = gtk::SingleSelection::new(Some(form.hits.clone()));
    selection.set_autoselect(true);
    selection.set_can_unselect(false);

    let (window, name_entry) = modal(parent, &t!("search.title"), (680, 560), |dialog| {
        {
            let form = Rc::clone(&form);
            dialog
                .window()
                .every(POLL_INTERVAL, move || poll_search(&form));
        }
        {
            let form = Rc::clone(&form);
            dialog.window().on_close_request(move || {
                form.cancel();
                false
            });
        }

        let name_entry = dialog.content(|| {
            paragraph(t!("search.description"));
            if !form.search_allowed {
                hint(t!("search.not_local"));
            }

            let name_entry = search_field(
                t!("search.name_label"),
                &t!("search.name_placeholder"),
                &form.name,
                &form,
            );
            search_field(
                t!("search.exclude_label"),
                &t!("search.exclude_placeholder"),
                &form.exclude,
                &form,
            );
            search_field(t!("search.location_label"), "", &form.location, &form);

            hbox(16, |_| {
                let option = |label, value| {
                    check(label, value, |check| check.bind_sensitive(&form.editable));
                };
                option(t!("search.include_subfolders"), &form.include_subfolders);
                option(t!("search.case_sensitive"), &form.case_sensitive);
                option(t!("search.use_regex"), &form.use_regex);
            });

            scrolled(|scroller| {
                scroller.set_hexpand(true);
                scroller.set_vexpand(true);
                scroller.css("panel-scroller");
                results_table(&selection, on_action);
            });

            hbox(8, |_| {
                spinner(|spinner| {
                    spinner.bind_spinning(&form.searching);
                    spinner.bind_visible(&form.searching);
                });
                text(&form.status, |label| {
                    label.set_hexpand(true);
                    label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
                    label.css("dim-label");
                });
            });

            name_entry
        });

        dialog.actions(|| {
            button(t!("search.start"), |button| {
                button.css("suggested-action");
                button.bind_sensitive(&form.editable);
                let form = Rc::clone(&form);
                button.on_click(move || start_search(&form));
            });
            button(t!("search.stop"), |button| {
                button.bind_sensitive(&form.searching);
                let form = Rc::clone(&form);
                button.on_click(move || stop_search(&form));
            });
            dialog.close_button(t!("common.close"));
        });

        name_entry
    });

    present_and_focus(&window, name_entry.widget());
}

/// The search form, the results and the running search.
struct SearchForm {
    name: Property<String>,
    exclude: Property<String>,
    location: Property<String>,
    include_subfolders: Property<bool>,
    case_sensitive: Property<bool>,
    use_regex: Property<bool>,
    status: Property<String>,
    searching: Property<bool>,
    /// The form takes input: the location is local and no search is running.
    editable: ReadOnly<bool>,
    search_allowed: bool,
    show_hidden_files: bool,
    hits: gio::ListStore,
    found: Cell<usize>,
    cancel_flag: RefCell<Option<Arc<AtomicBool>>>,
    /// What the worker reports, until it is done. It outlives "Stop", so the
    /// hits and the count of a cancelled search still arrive; only the next
    /// search drops it, which is what keeps stale events out of its results.
    events: RefCell<Option<Receiver<SearchEvent>>>,
}

impl SearchForm {
    fn new(local_root: Option<PathBuf>, show_hidden_files: bool) -> Self {
        let search_allowed = local_root.is_some();
        let location = local_root
            .map(|root| root.display().to_string())
            .unwrap_or_default();
        let searching = Property::new(false);
        Self {
            name: Property::new(String::new()),
            exclude: Property::new(String::new()),
            location: Property::new(location),
            include_subfolders: Property::new(true),
            case_sensitive: Property::new(false),
            use_regex: Property::new(false),
            status: Property::new(t!("search.status_idle").into_owned()),
            editable: searching.map(move |active| search_allowed && !active),
            searching,
            search_allowed,
            show_hidden_files,
            hits: gio::ListStore::new::<SearchHitObject>(),
            found: Cell::new(0),
            cancel_flag: RefCell::new(None),
            events: RefCell::new(None),
        }
    }

    fn cancel(&self) {
        if let Some(flag) = self.cancel_flag.borrow_mut().take() {
            flag.store(true, Ordering::Relaxed);
        }
    }
}

fn start_search(form: &SearchForm) {
    if !form.search_allowed || form.searching.get() {
        return;
    }

    let root_text = form.location.get();
    let root = PathBuf::from(root_text.trim());
    if root_text.trim().is_empty() || !root.is_dir() {
        form.status.set(t!("search.invalid_location").into_owned());
        return;
    }

    form.cancel();

    form.hits.remove_all();
    form.found.set(0);
    form.events.borrow_mut().take();

    let flag = Arc::new(AtomicBool::new(false));
    *form.cancel_flag.borrow_mut() = Some(Arc::clone(&flag));

    let options = SearchOptions {
        root,
        name_mask: form.name.get(),
        exclude_mask: form.exclude.get(),
        include_subfolders: form.include_subfolders.get(),
        case_sensitive: form.case_sensitive.get(),
        use_regex: form.use_regex.get(),
        show_hidden_files: form.show_hidden_files,
    };

    if let Err(error) = compile_search(&options) {
        *form.cancel_flag.borrow_mut() = None;
        form.status
            .set(t!("search.invalid_pattern", error = error).into_owned());
        return;
    }

    form.searching.set(true);
    form.status.set(
        t!(
            "search.status_searching",
            path = options.root.display().to_string(),
            count = 0
        )
        .into_owned(),
    );

    *form.events.borrow_mut() = Some(spawn_search(options, flag));
}

fn stop_search(form: &SearchForm) {
    form.cancel();
    form.searching.set(false);
    form.status
        .set(t!("search.status_cancelled", count = form.found.get()).into_owned());
}

/// Takes in everything the worker reported since the last tick.
fn poll_search(form: &SearchForm) {
    loop {
        let received = match form.events.borrow().as_ref() {
            Some(events) => events.try_recv(),
            None => return,
        };
        let listening = match received {
            Ok(event) => apply_event(form, event),
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => false,
        };
        if !listening {
            form.events.borrow_mut().take();
            return;
        }
    }
}

/// Applies one event; returns whether more are to come.
fn apply_event(form: &SearchForm, event: SearchEvent) -> bool {
    match event {
        SearchEvent::Progress { path, found } => {
            form.found.set(found);
            form.status.set(
                t!(
                    "search.status_searching",
                    path = path.display().to_string(),
                    count = found
                )
                .into_owned(),
            );
            true
        }
        SearchEvent::Hits(hits) => {
            for hit in hits {
                form.hits.append(&SearchHitObject::new(hit));
            }
            true
        }
        SearchEvent::Failed { message } => {
            *form.cancel_flag.borrow_mut() = None;
            form.searching.set(false);
            form.status
                .set(t!("search.invalid_pattern", error = message).into_owned());
            false
        }
        SearchEvent::Finished {
            found,
            errors,
            cancelled,
        } => {
            form.found.set(found);
            *form.cancel_flag.borrow_mut() = None;
            form.searching.set(false);
            let status = if cancelled {
                t!("search.status_cancelled", count = found)
            } else if errors > 0 {
                t!(
                    "search.status_done_with_errors",
                    count = found,
                    errors = errors
                )
            } else {
                t!("search.status_done", count = found)
            };
            form.status.set(status.into_owned());
            false
        }
    }
}

fn go_selected(selection: &gtk::SingleSelection, on_action: &impl Fn(SearchDialogAction)) {
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
    on_action(SearchDialogAction::GoTo {
        directory,
        focus_name,
    });
}

/// A labelled entry of the form; Enter in it starts the search.
fn search_field(
    label: impl IntoText,
    placeholder: &str,
    value: &Property<String>,
    form: &Rc<SearchForm>,
) -> Ctx<gtk::Entry> {
    let mut input = None;
    hbox(8, |_| {
        text(label, |label| label.set_width_chars(12));
        let entry = field(placeholder, value);
        entry.set_sensitive(form.search_allowed);
        // A running search only locks the text. Disabling the focused entry
        // leaves its Windows IME filter behind for a context GTK then frees.
        entry.bind(&form.searching, |entry, searching| {
            entry.set_editable(!*searching);
        });
        entry.css_if("dim-label", &form.searching);
        let form = Rc::clone(form);
        entry.on_activate(move || start_search(&form));
        input = Some(entry);
    });
    input.expect("search field composed")
}

fn results_table(
    selection: &gtk::SingleSelection,
    on_action: impl Fn(SearchDialogAction) + 'static,
) -> Table {
    column_view(selection, |table| {
        table.set_reorderable(false);
        table.set_hexpand(true);
        table.set_vexpand(true);
        table.css("data-table");
        table.css("search-results");

        text_column(table, t!("column.name"), 160, true, SearchHitObject::name);
        text_column(
            table,
            t!("column.type"),
            70,
            false,
            SearchHitObject::type_label,
        );
        text_column(
            table,
            t!("search.column_path"),
            240,
            true,
            SearchHitObject::parent_display,
        );
        text_column(
            table,
            t!("column.size"),
            90,
            false,
            SearchHitObject::size_label,
        );
        text_column(
            table,
            t!("column.modified"),
            130,
            false,
            SearchHitObject::modified_label,
        );

        let selection = selection.clone();
        table.on_row_activated(move |_| go_selected(&selection, &on_action));
    })
}

fn text_column(
    table: &Table,
    title: impl IntoText,
    width: i32,
    expand: bool,
    value: impl Fn(&SearchHitObject) -> String + 'static,
) {
    let column = table.column(
        title,
        || {
            text("", |label| {
                label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                label.set_hexpand(true);
            })
            .widget()
            .clone()
        },
        move |label: &gtk::Label, hit: &SearchHitObject| label.set_text(&value(hit)),
    );
    column.set_resizable(true);
    column.set_expand(expand);
    column.set_fixed_width(width);
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
