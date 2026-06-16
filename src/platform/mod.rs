pub mod icons;
pub mod open;

#[cfg(not(target_os = "windows"))]
pub mod unix;

#[cfg(target_os = "windows")]
pub mod windows;

pub use icons::icon_for_entry;
pub use open::open_path;

#[cfg(target_os = "windows")]
pub use windows::available_roots;

#[cfg(not(target_os = "windows"))]
pub use unix::available_roots;
