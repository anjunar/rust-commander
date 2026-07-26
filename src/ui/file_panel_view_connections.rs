use std::rc::Rc;

use gtk::{gdk, prelude::*};

use crate::domain::sorting::{SortColumn, SortDirection};

use super::{
    helpers::find_row_position,
    FilePanelView, RootSelectorChoice,
};

impl FilePanelView {
    pub fn connect_root_changed<F>(&self, f: F)
    where
        F: Fn(RootSelectorChoice) + 'static,
    {
        let ignore_roots = Rc::clone(&self.ignore_roots);
        let sftp_item_index = Rc::clone(&self.sftp_item_index);
        let last_drive_index = Rc::clone(&self.last_drive_index);
        let root_dropdown = self.root_dropdown.clone();
        self.root_dropdown.connect_selected_notify(move |dropdown| {
            if ignore_roots.get() {
                return;
            }
            let selected = dropdown.selected();
            if selected == gtk::INVALID_LIST_POSITION {
                return;
            }
            if sftp_item_index.get() == Some(selected) {
                ignore_roots.set(true);
                if let Some(drive_index) = last_drive_index.get() {
                    root_dropdown.set_selected(drive_index);
                }
                ignore_roots.set(false);
                f(RootSelectorChoice::Sftp);
                return;
            }
            last_drive_index.set(Some(selected));
            f(RootSelectorChoice::Drive(selected as usize));
        });
    }

    pub fn connect_secondary_click<F>(&self, f: F)
    where
        F: Fn(Option<usize>, f64, f64) + 'static,
    {
        let root = self.root.clone();
        let column_view = self.column_view.clone();
        let gesture = gtk::GestureClick::new();
        gesture.set_button(gdk::BUTTON_SECONDARY);
        gesture.connect_pressed(move |_, _, x, y| {
            let clicked_position = column_view
                .pick(x, y, gtk::PickFlags::DEFAULT)
                .and_then(|widget| find_row_position(&widget, &column_view));
            let point = column_view
                .compute_point(&root, &gtk::graphene::Point::new(x as f32, y as f32))
                .unwrap_or_else(|| gtk::graphene::Point::new(x as f32, y as f32));
            f(clicked_position, point.x() as f64, point.y() as f64);
        });
        self.column_view.add_controller(gesture);
    }

    pub fn apply_sort(&self, column: SortColumn, direction: SortDirection) {
        let Some(binding) = self
            .columns
            .iter()
            .find(|binding| binding.sort_column == column)
        else {
            return;
        };
        let sort_type = match direction {
            SortDirection::Ascending => gtk::SortType::Ascending,
            SortDirection::Descending => gtk::SortType::Descending,
        };
        let was_ignoring_sort = self.ignore_sort.replace(true);
        self.column_view
            .sort_by_column(Some(&binding.column), sort_type);
        self.last_sort
            .borrow_mut()
            .replace((column, sort_type));
        self.ignore_sort.set(was_ignoring_sort);
    }

    pub fn connect_sort_changed<F>(&self, f: F)
    where
        F: Fn(SortColumn, gtk::SortType) + 'static,
    {
        let Some(sorter) = self
            .column_view
            .sorter()
            .and_then(|sorter| sorter.downcast::<gtk::ColumnViewSorter>().ok())
        else {
            return;
        };
        let columns = self.columns.clone();
        let ignore_sort = Rc::clone(&self.ignore_sort);
        let last_sort = Rc::clone(&self.last_sort);
        sorter.connect_changed(move |sorter, _| {
            if ignore_sort.get() {
                return;
            }
            let Some(primary_column) = sorter.primary_sort_column() else {
                return;
            };
            let direction = sorter.primary_sort_order();
            if let Some(binding) = columns
                .iter()
                .find(|binding| binding.column == primary_column)
            {
                let next_sort = (binding.sort_column, direction);
                if last_sort.borrow().as_ref() == Some(&next_sort) {
                    return;
                }
                last_sort.borrow_mut().replace(next_sort);
                f(binding.sort_column, direction);
            }
        });
    }
}
