pub mod application;
pub mod archive;
pub mod config;
pub mod domain;
pub mod fs;
pub mod platform;
pub mod ui;
pub mod viewer;

pub use ui::gtk_app::run;
