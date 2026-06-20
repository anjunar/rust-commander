use std::{cell::RefCell, rc::Rc};

use gtk::glib;
use rust_i18n::t;

use crate::{
    application::{ActivePanel, Commander},
    domain::sorting::SortDirection,
    presentation,
    ui::commander_view::CommanderView,
};

use super::{hosts::ViewHost, navigation_controller::NavigationController};

#[derive(Clone)]
pub struct PanelWiring {
    host: Rc<dyn ViewHost>,
    commander: Rc<RefCell<Commander>>,
    navigation: NavigationController,
    context_menu_handler: Rc<dyn Fn(ActivePanel, Option<usize>, f64, f64)>,
    remote_connect_handler: Rc<dyn Fn(ActivePanel)>,
}

impl PanelWiring {
    pub fn new(
        host: Rc<dyn ViewHost>,
        commander: Rc<RefCell<Commander>>,
        navigation: NavigationController,
        context_menu_handler: Rc<dyn Fn(ActivePanel, Option<usize>, f64, f64)>,
        remote_connect_handler: Rc<dyn Fn(ActivePanel)>,
    ) -> Self {
        Self {
            host,
            commander,
            navigation,
            context_menu_handler,
            remote_connect_handler,
        }
    }

    pub fn connect_panels(&self, commander_view: &CommanderView) {
        for panel in [ActivePanel::Left, ActivePanel::Right] {
            let panel_view = commander_view.panel(panel);

            {
                let host = Rc::clone(&self.host);
                let commander = Rc::clone(&self.commander);
                panel_view.connect_selection_changed(move |indices| {
                    let update = {
                        let mut commander = commander.borrow_mut();
                        commander.select_indices(panel, indices)
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
                panel_view.connect_open_key(move || {
                    let navigation = navigation.clone();
                    glib::idle_add_local_once(move || {
                        navigation.start_selected_navigation(panel);
                    });
                });
            }

            {
                let navigation = self.navigation.clone();
                panel_view.connect_root_changed(move |index| {
                    let navigation = navigation.clone();
                    glib::idle_add_local_once(move || {
                        navigation.start_root_navigation(panel, index);
                    });
                });
            }

            {
                let remote_connect_handler = Rc::clone(&self.remote_connect_handler);
                panel_view.connect_remote_connect(move || {
                    let remote_connect_handler = Rc::clone(&remote_connect_handler);
                    glib::idle_add_local_once(move || {
                        remote_connect_handler(panel);
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
                panel_view.connect_sort_changed(move |column, sort_type| {
                    let host = Rc::clone(&host);
                    let commander = Rc::clone(&commander);
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
