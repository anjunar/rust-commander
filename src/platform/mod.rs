pub mod icons;
pub mod open;
pub mod terminal;
pub mod tray;

#[cfg(not(target_os = "windows"))]
pub mod unix;

#[cfg(target_os = "windows")]
pub mod windows;

pub use icons::icon_for_entry;
pub use open::open_path;
pub use terminal::open_console;

#[cfg(target_os = "windows")]
pub use windows::available_roots;

#[cfg(not(target_os = "windows"))]
pub use unix::available_roots;
