use std::{cell::RefCell, rc::Rc};

use super::{ListChange, ListProperty};

fn changes<T: Clone + 'static>(
    list: &ListProperty<T>,
) -> (Rc<RefCell<Vec<ListChange<T>>>>, super::Disposable) {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&seen);
    let subscription = list.observe_changes(move |change| sink.borrow_mut().push(change.clone()));
    (seen, subscription)
}

#[test]
fn push_insert_remove_report_positions_and_elements() {
    let list = ListProperty::new(vec!["a"]);
    let (seen, _subscription) = changes(&list);

    list.push("c");
    list.insert(1, "b");
    let removed = list.remove(0);

    assert_eq!(removed, "a");
    assert_eq!(list.get(), vec!["b", "c"]);
    assert_eq!(
        *seen.borrow(),
        vec![
            ListChange::Insert {
                index: 1,
                element: "c"
            },
            ListChange::Insert {
                index: 1,
                element: "b"
            },
            ListChange::Remove {
                index: 0,
                element: "a"
            },
        ]
    );
}

#[test]
fn set_reports_old_and_new_and_ignores_equal_values() {
    let list = ListProperty::new(vec![1, 2]);
    let (seen, _subscription) = changes(&list);

    list.set(1, 2);
    list.set(1, 3);

    assert_eq!(list.item(1), Some(3));
    assert_eq!(
        *seen.borrow(),
        vec![ListChange::Update {
            index: 1,
            old: 2,
            new: 3
        }]
    );
}

#[test]
fn clear_and_replace_all_report_whole_list_changes() {
    let list = ListProperty::new(vec![1]);
    let (seen, _subscription) = changes(&list);

    list.clear();
    list.clear();
    list.replace_all(vec![7, 8]);

    assert!(list.len() == 2);
    assert_eq!(
        *seen.borrow(),
        vec![ListChange::Clear, ListChange::Reset { items: vec![7, 8] }]
    );
}

#[test]
fn observe_delivers_full_content_now_and_after_changes() {
    let list = ListProperty::new(vec!["x".to_string()]);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&seen);

    let _subscription = list.observe(move |items| sink.borrow_mut().push(items.to_vec()));
    list.push("y".to_string());

    assert_eq!(
        *seen.borrow(),
        vec![
            vec!["x".to_string()],
            vec!["x".to_string(), "y".to_string()]
        ]
    );
}

#[test]
fn map_derives_a_value_from_the_list() {
    let list = ListProperty::new(vec![1, 2, 3]);
    let count = list.map(|items| items.len());

    assert_eq!(count.get(), 3);
    list.remove(0);
    assert_eq!(count.get(), 2);
}

#[test]
fn disposed_subscription_hears_nothing() {
    let list = ListProperty::<u8>::default();
    let (seen, subscription) = changes(&list);

    subscription.dispose();
    list.push(1);

    assert!(seen.borrow().is_empty());
}

#[test]
#[should_panic(expected = "propagation cycle")]
fn changing_the_list_from_its_own_observer_panics() {
    let list = ListProperty::new(vec![0]);
    let echo = list.clone();
    let _subscription = list.observe_changes(move |_| echo.push(1));

    list.push(1);
}
