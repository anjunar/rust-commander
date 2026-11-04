use std::{cell::Cell, rc::Rc};

use super::{localized, localized_with, relocalize};
use crate::ui::dsl::Property;

#[test]
fn localized_text_is_produced_again_after_relocalize() {
    let calls = Rc::new(Cell::new(0));
    let text = {
        let calls = Rc::clone(&calls);
        localized(move || {
            calls.set(calls.get() + 1);
            format!("call {}", calls.get())
        })
    };
    let seen = Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = Rc::clone(&seen);
    let _subscription = text.observe(move |value| sink.borrow_mut().push(value.clone()));

    relocalize();

    assert_eq!(
        *seen.borrow(),
        vec!["call 1".to_string(), "call 2".to_string()]
    );
}

#[test]
fn localized_with_follows_value_and_locale() {
    let name = Property::new("a".to_string());
    let text = localized_with(&name, |name| format!("hello {name}"));
    let seen = Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = Rc::clone(&seen);
    let _subscription = text.observe(move |value| sink.borrow_mut().push(value.clone()));

    name.set("b".to_string());
    relocalize();

    assert_eq!(
        *seen.borrow(),
        vec![
            "hello a".to_string(),
            "hello b".to_string(),
            "hello b".to_string()
        ]
    );
}

#[test]
fn tr_translates_a_key() {
    let text = tr!("common.cancel");

    assert!(!text.get().is_empty());
    assert_ne!(text.get(), "common.cancel");
}
