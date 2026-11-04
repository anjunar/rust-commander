use std::{cell::RefCell, rc::Rc};

use super::{CompositeDisposable, Disposable, IntoText, Property, ReadOnly};

fn recorder<T: Clone + 'static>() -> (Rc<RefCell<Vec<T>>>, impl Fn(&T) + 'static) {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&seen);
    (seen, move |value: &T| sink.borrow_mut().push(value.clone()))
}

#[test]
fn observe_delivers_current_value_and_changes() {
    let property = Property::new(1);
    let (seen, observer) = recorder();

    let _subscription = property.observe(observer);
    property.set(2);
    property.set(3);

    assert_eq!(*seen.borrow(), vec![1, 2, 3]);
}

#[test]
fn observe_without_initial_skips_current_value() {
    let property = Property::new(1);
    let (seen, observer) = recorder();

    let _subscription = property.observe_without_initial(observer);
    property.set(2);

    assert_eq!(*seen.borrow(), vec![2]);
}

#[test]
fn set_ignores_equal_values_but_set_always_does_not() {
    let property = Property::new("a".to_string());
    let (seen, observer) = recorder();
    let _subscription = property.observe_without_initial(observer);

    property.set("a".to_string());
    assert!(seen.borrow().is_empty());

    property.set_always("a".to_string());
    assert_eq!(*seen.borrow(), vec!["a".to_string()]);
}

#[test]
fn update_derives_from_current_value() {
    let property = Property::new(10);

    property.update(|value| value * 2);

    assert_eq!(property.get(), 20);
}

#[test]
fn disposed_subscription_receives_nothing() {
    let property = Property::new(0);
    let (seen, observer) = recorder();

    let subscription = property.observe_without_initial(observer);
    subscription.dispose();
    property.set(1);

    assert!(seen.borrow().is_empty());
}

#[test]
fn listener_disposed_during_notification_is_skipped() {
    let property = Property::new(0);
    let second: Rc<RefCell<Option<Disposable>>> = Rc::new(RefCell::new(None));
    let (seen, observer) = recorder();

    let _first = {
        let second = Rc::clone(&second);
        property.observe_without_initial(move |_| {
            let subscription = second.borrow_mut().take();
            if let Some(subscription) = subscription {
                subscription.dispose();
            }
        })
    };
    *second.borrow_mut() = Some(property.observe_without_initial(observer));

    property.set(1);

    assert!(seen.borrow().is_empty());
}

#[test]
fn map_follows_the_source() {
    let count = Property::new(1);
    let label = count.map(|value| format!("Count: {value}"));
    let (seen, observer) = recorder();

    assert_eq!(label.get(), "Count: 1");
    let _subscription = label.observe(observer);
    count.set(2);

    assert_eq!(
        *seen.borrow(),
        vec!["Count: 1".to_string(), "Count: 2".to_string()]
    );
}

#[test]
fn map_chains() {
    let count = Property::new(2);
    let doubled_text = count.map(|value| value * 2).map(|value| value.to_string());

    assert_eq!(doubled_text.get(), "4");
    count.set(5);
    assert_eq!(doubled_text.get(), "10");
}

#[test]
fn constant_notifies_exactly_once() {
    let constant = ReadOnly::constant(7);
    let (seen, observer) = recorder();

    let subscription = constant.observe(observer);

    assert_eq!(*seen.borrow(), vec![7]);
    assert!(subscription.is_disposed());
}

#[test]
fn into_text_accepts_plain_and_reactive_strings() {
    let plain = "fixed".into_text();
    let property = Property::new("live".to_string());
    let reactive = (&property).into_text();

    assert_eq!(plain.get(), "fixed");
    assert_eq!(reactive.get(), "live");
    property.set("changed".to_string());
    assert_eq!(reactive.get(), "changed");
}

#[test]
fn reset_returns_to_default_and_tracks_dirty() {
    let property = Property::with_default(5, 1);
    assert!(property.is_dirty());

    property.reset();

    assert_eq!(property.get(), 1);
    assert!(!property.is_dirty());

    property.set_default(9);
    assert!(property.is_dirty());
    property.reset();
    assert_eq!(property.get(), 9);
}

#[test]
#[should_panic(expected = "propagation cycle")]
fn setting_a_property_from_its_own_observer_panics() {
    let property = Property::new(0);
    let echo = property.clone();
    let _subscription = property.observe_without_initial(move |value| echo.set(value + 1));

    property.set(1);
}

#[test]
fn subscription_does_not_keep_the_property_alive() {
    let property = Property::new(0);
    let subscription = property.observe(|_| {});

    drop(property);
    subscription.dispose();

    assert!(subscription.is_disposed());
}

#[test]
fn disposable_runs_once() {
    let runs = Rc::new(RefCell::new(0));
    let disposable = {
        let runs = Rc::clone(&runs);
        Disposable::new(move || *runs.borrow_mut() += 1)
    };

    disposable.dispose();
    disposable.dispose();

    assert_eq!(*runs.borrow(), 1);
    assert!(disposable.is_disposed());
}

#[test]
fn composite_disposes_members_and_late_additions() {
    let composite = CompositeDisposable::new();
    let early = Disposable::new(|| {});
    composite.add(early.clone());
    assert_eq!(composite.len(), 1);

    composite.dispose();
    assert!(early.is_disposed());
    assert!(composite.is_disposed());
    assert!(composite.is_empty());

    let late = Disposable::new(|| {});
    composite.add(late.clone());
    assert!(late.is_disposed());
}

#[test]
fn composite_remove_keeps_member_alive() {
    let composite = CompositeDisposable::new();
    let kept = Disposable::new(|| {});
    composite.add(kept.clone());

    composite.remove(&kept);
    composite.dispose();

    assert!(!kept.is_disposed());
}

#[test]
fn combine_recomputes_when_either_side_changes() {
    let first = Property::new(1);
    let second = Property::new(10);
    let sum = first.combine(&second.read_only(), |a, b| a + b);
    let (seen, observer) = recorder();

    let subscription = sum.observe(observer);
    first.set(2);
    second.set(20);
    subscription.dispose();
    first.set(3);

    assert_eq!(*seen.borrow(), vec![11, 12, 22]);
    assert_eq!(sum.get(), 23);
}
