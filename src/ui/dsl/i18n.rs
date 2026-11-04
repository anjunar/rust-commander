//! Reactive translations: the counterpart of scalajs-ui's message state.
//!
//! Every localized text is a [`ReadOnly`] derived from one locale revision.
//! After the locale changes, [`relocalize`] bumps the revision and every
//! label, button and column title built from [`tr!`] or [`localized`]
//! re-renders itself. Nothing has to walk the widget tree to relabel it.

use super::state::{IntoReadOnly, Property, ReadOnly};

thread_local! {
    static REVISION: Property<u64> = Property::new(0);
}

/// A text that is produced again whenever the locale changes.
pub fn localized(text: impl Fn() -> String + 'static) -> ReadOnly<String> {
    REVISION.with(|revision| revision.map(move |_| text()))
}

/// A text that depends on a value and on the locale, for messages with arguments.
pub fn localized_with<T: Clone + 'static>(
    value: impl IntoReadOnly<T>,
    text: impl Fn(&T) -> String + 'static,
) -> ReadOnly<String> {
    let value = value.into_read_only();
    REVISION.with(|revision| revision.combine(&value, move |_, value| text(value)))
}

/// Re-renders every localized text. Call it after `rust_i18n::set_locale`.
pub fn relocalize() {
    REVISION.with(|revision| revision.update(|value| value.wrapping_add(1)));
}

/// `t!` as a reactive text: same arguments, but the result follows the locale.
///
/// ```ignore
/// button(tr!("common.save"), |b| ...);
/// text(tr!("terminal.panel_path", path = "/tmp"), |_| {});
/// ```
macro_rules! tr {
    ($($args:tt)*) => {
        $crate::ui::dsl::localized(move || ::rust_i18n::t!($($args)*).into_owned())
    };
}

pub(crate) use tr;

#[cfg(test)]
#[path = "../../../tests/unit/ui_dsl_i18n_tests.rs"]
mod tests;
