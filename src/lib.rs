#[macro_use]
extern crate rust_i18n;

i18n!("locales", fallback = "en");

mod application;
mod archive;
mod config;
mod domain;
mod fs;
mod i18n;
mod platform;
mod presentation;
mod remote;
mod ui;
mod viewer;

pub use ui::gtk_app::run;
