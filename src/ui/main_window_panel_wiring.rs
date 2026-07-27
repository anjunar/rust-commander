use std::{cell::RefCell, rc::Rc};

use gtk::glib;
use rust_i18n::t;

use crate::{
    application::{ActivePanel, Commander, ConfigStore},
    domain::sorting::SortDirection,
    presentation,
    ui::{
        commander_view::CommanderView,
        file_panel_view::{PanelKeyNavigation, RootSelectorChoice},
    },
};

use super::{
    hosts::ViewHost, navigation_controller::NavigationController,
    window_state_controller::persist_pane_session,
};

#[derive(Clone)]
pub struct PanelWiring {
    host: Rc<dyn ViewHost>,
    commander: Rc<RefCell<Commander>>,
    navigation: NavigationController,
    context_menu_handler: Rc<dyn Fn(ActivePanel, Option<usize>, f64, f64)>,
    remote_connect_handler: Rc<dyn Fn(ActivePanel)>,
    config_store: ConfigStore,
}

impl PanelWiring {
    pub fn new(
        host: Rc<dyn ViewHost>,
        commander: Rc<RefCell<Commander>>,
        navigation: NavigationController,
        context_menu_handler: Rc<dyn Fn(ActivePanel, Option<usize>, f64, f64)>,
        remote_connect_handler: Rc<dyn Fn(ActivePanel)>,
        config_store: ConfigStore,
    ) -> Self {
        Self {
            host,
            commander,
            navigation,
            context_menu_handler,
            remote_connect_handler,
            config_store,
        }
    }

    pub fn connect_panels(&self, commander_view: &CommanderView) {
        for panel in [ActivePanel::Left, ActivePanel::Right] {
            let panel_view = commander_view.panel(panel);

            {
                let navigation = self.navigation.clone();
                panel_view.connect_tab_bar(
                    {
                        let host = Rc::clone(&self.host);
                        let commander = Rc::clone(&self.commander);
                        let navigation = navigation.clone();
                        move |index| {
                            let (update, needs_load) = {
                                let mut commander = commander.borrow_mut();
                                let update = commander.activate_tab(panel, index);
                                let needs_load = commander.state().panel(panel).entries.is_empty();
                                (update, needs_load)
                            };
                            host.apply_update(update);
                            if needs_load {
                                let request = {
                                    let commander = commander.borrow();
                                    crate::application::refresh_request(
                                        &commander,
                                        panel,
                                        rust_i18n::t!("status.loading_directory").into_owned(),
                                    )
                                };
                                navigation.start_directory_load(request);
                            }
                        }
                    },
                    {
                        let host = Rc::clone(&self.host);
                        let commander = Rc::clone(&self.commander);
                        let navigation = navigation.clone();
                        move || {
                            let (update, request) = {
                                let mut commander = commander.borrow_mut();
                                let update = commander.open_tab(panel);
                                let request = crate::application::refresh_request(
                                    &commander,
                                    panel,
                                    rust_i18n::t!("status.loading_directory").into_owned(),
                                );
                                (update, request)
                            };
                            host.apply_update(update);
                            navigation.start_directory_load(request);
                        }
                    },
                );
            }

            {
                let host = Rc::clone(&self.host);
                let commander = Rc::clone(&self.commander);
                panel_view.connect_selection_changed(move |index| {
                    let update = {
                        let mut commander = commander.borrow_mut();
                        commander.set_cursor(panel, index)
                    };
                    host.apply_update(update);
                });
            }

            {
                let host = Rc::clone(&self.host);
                let commander = Rc::clone(&self.commander);
                panel_view.connect_focus_enter(move || {
                    activate_panel_if_needed(&host, &commander, panel);
                });
            }

            {
                let host = Rc::clone(&self.host);
                let commander = Rc::clone(&self.commander);
                panel_view.connect_primary_click(move || {
                    activate_panel_if_needed(&host, &commander, panel);
                });
            }

            {
                let navigation = self.navigation.clone();
                panel_view.connect_activate(move |index| {
                    let navigation = navigation.clone();
                    glib::idle_add_local_once(move || {
                        navigation.select_single_and_start(panel, index);
                    });
                });
            }

            {
                let navigation = self.navigation.clone();
                let host = Rc::clone(&self.host);
                let commander = Rc::clone(&self.commander);
                let host_for_search = Rc::clone(&self.host);
                let commander_for_search = Rc::clone(&self.commander);
                let context_menu_handler = Rc::clone(&self.context_menu_handler);
                let commander_for_menu = Rc::clone(&self.commander);
                panel_view.connect_open_key(
                    move |action| match action {
                        PanelKeyNavigation::ToggleMark => {
                            {
                                let mut commander = commander.borrow_mut();
                                let _ = commander.toggle_mark(panel);
                            }
                            host.apply_panel_selection(panel);
                        }
                        PanelKeyNavigation::SpaceAction => {
                            navigation.handle_space_action(panel);
                        }
                        PanelKeyNavigation::InvertMarks => {
                            {
                                let mut commander = commander.borrow_mut();
                                let _ = commander.invert_marks(panel);
                            }
                            host.apply_panel_selection(panel);
                        }
                        PanelKeyNavigation::ContextMenu => {
                            let context_menu_handler = Rc::clone(&context_menu_handler);
                            let commander_for_menu = Rc::clone(&commander_for_menu);
                            glib::idle_add_local_once(move || {
                                let index = commander_for_menu
                                    .borrow()
                                    .state()
                                    .panel(panel)
                                    .cursor_index();
                                context_menu_handler(panel, index, 32.0, 48.0);
                            });
                        }
                        other => {
                            let navigation = navigation.clone();
                            glib::idle_add_local_once(move || match other {
                                PanelKeyNavigation::ActivateSelected => {
                                    navigation.start_selected_navigation(panel);
                                }
                                PanelKeyNavigation::EnterSelected => {
                                    navigation.start_enter_selected_directory(panel);
                                }
                                PanelKeyNavigation::GoParent => {
                                    navigation.start_parent_navigation(panel);
                                }
                                PanelKeyNavigation::ToggleMark
                                | PanelKeyNavigation::SpaceAction
                                | PanelKeyNavigation::InvertMarks
                                | PanelKeyNavigation::ContextMenu => {}
                            });
                        }
                    },
                    move |query| {
                        if query.is_empty() {
                            let update = commander_for_search
                                .borrow_mut()
                                .set_status(t!("status.ready").into_owned());
                            host_for_search.apply_update(update);
                            return false;
                        }
                        let (update, matched) =
                            commander_for_search.borrow_mut().quick_search(panel, query);
                        if matched {
                            host_for_search.apply_update(update);
                        }
                        matched
                    },
                );
            }

            {
                let navigation = self.navigation.clone();
                let remote_connect_handler = Rc::clone(&self.remote_connect_handler);
                panel_view.connect_root_changed(move |choice| {
                    let navigation = navigation.clone();
                    let remote_connect_handler = Rc::clone(&remote_connect_handler);
                    glib::idle_add_local_once(move || match choice {
                        RootSelectorChoice::Drive(index) => {
                            navigation.start_root_navigation(panel, index);
                        }
                        RootSelectorChoice::Sftp => {
                            remote_connect_handler(panel);
                        }
                    });
                });
            }

            {
                let context_menu_handler = Rc::clone(&self.context_menu_handler);
                panel_view.connect_secondary_click(move |clicked_index, x, y| {
                    let context_menu_handler = Rc::clone(&context_menu_handler);
                    glib::idle_add_local_once(move || {
                        context_menu_handler(panel, clicked_index, x, y);
                    });
                });
            }

            {
                let host = Rc::clone(&self.host);
                let commander = Rc::clone(&self.commander);
                let config_store = self.config_store.clone();
                panel_view.connect_sort_changed(move |column, sort_type| {
                    let host = Rc::clone(&host);
                    let commander = Rc::clone(&commander);
                    let config_store = config_store.clone();
                    glib::idle_add_local_once(move || {
                        let direction = match sort_type {
                            gtk::SortType::Descending => SortDirection::Descending,
                            _ => SortDirection::Ascending,
                        };
                        let status = t!(
                            "status.sorted_panel",
                            panel = presentation::panel_label(panel),
                            column = presentation::sort_column_label(column)
                        )
                        .into_owned();
                        let update = {
                            let mut commander = commander.borrow_mut();
                            commander.sort_panel(panel, column, direction, status)
                        };
                        host.apply_update(update);
                        if let Err(error) = persist_pane_session(&commander.borrow(), &config_store)
                        {
                            eprintln!("Could not save pane session: {error}");
                        }
                    });
                });
            }
        }
    }
}

fn activate_panel_if_needed(
    host: &Rc<dyn ViewHost>,
    commander: &Rc<RefCell<Commander>>,
    panel: ActivePanel,
) {
    let update = {
        let mut commander = commander.borrow_mut();
        if commander.state().active_panel == panel {
            return;
        }
        commander.set_active_panel(panel)
    };
    host.apply_update(update);
}
