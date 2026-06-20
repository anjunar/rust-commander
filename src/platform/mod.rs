pub mod assets;
pub mod context_menu;
pub mod icons;
pub mod open;
pub mod terminal;
pub mod tray;
pub mod window_geometry;

#[cfg(not(target_os = "windows"))]
pub mod unix;
#[cfg(all(unix, not(target_os = "macos")))]
pub mod x11_window_icon;

#[cfg(target_os = "windows")]
pub mod windows;

pub use context_menu::{show_context_menu, ContextMenuRequest};
pub use icons::icon_for_entry;
pub use open::open_path;
pub use terminal::open_console;
pub use window_geometry::current_window_placement;
#[cfg(target_os = "windows")]
pub use window_geometry::restore_window_placement;

#[cfg(target_os = "windows")]
pub use windows::apply_runtime_window_icon;
#[cfg(target_os = "windows")]
pub use windows::available_roots;

#[cfg(not(target_os = "windows"))]
pub use unix::available_roots;
#[cfg(not(target_os = "windows"))]
pub use unix::{chmod_paths, chown_paths};
