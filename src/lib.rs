#[macro_use]
extern crate rust_i18n;

i18n!("locales", fallback = "en");

pub mod application;
pub mod archive;
pub mod config;
pub mod domain;
pub mod fs;
pub mod i18n;
pub mod platform;
pub mod ui;
pub mod viewer;

pub use ui::gtk_app::run;
