use std::{path::Path, path::PathBuf, rc::Rc};

use anyhow::Result;

use crate::domain::RootLocation;

pub trait PlatformPort {
    fn available_roots(&self) -> Vec<RootLocation>;
    fn open_path(&self, path: &Path) -> Result<()>;
    fn open_console(&self, path: &Path) -> Result<()>;
    fn show_context_menu(&self, directory: PathBuf, selected_paths: Vec<PathBuf>) -> Result<()>;
    #[cfg(not(target_os = "windows"))]
    fn chmod_paths(&self, paths: &[PathBuf], mode: &str, recursive: bool) -> Result<()>;
    #[cfg(not(target_os = "windows"))]
    fn chown_paths(&self, paths: &[PathBuf], owner_spec: &str, recursive: bool) -> Result<()>;
}

pub type SharedPlatformPort = Rc<dyn PlatformPort>;

pub fn system_platform_port() -> SharedPlatformPort {
    Rc::new(SystemPlatformPort)
}

struct SystemPlatformPort;

impl PlatformPort for SystemPlatformPort {
    fn available_roots(&self) -> Vec<RootLocation> {
        crate::platform::available_roots()
    }

    fn open_path(&self, path: &Path) -> Result<()> {
        crate::platform::open_path(path)
    }

    fn open_console(&self, path: &Path) -> Result<()> {
        crate::platform::open_console(path)
    }

    fn show_context_menu(&self, directory: PathBuf, selected_paths: Vec<PathBuf>) -> Result<()> {
        crate::platform::show_context_menu(&crate::platform::ContextMenuRequest {
            directory,
            selected_paths,
        })
    }

    #[cfg(not(target_os = "windows"))]
    fn chmod_paths(&self, paths: &[PathBuf], mode: &str, recursive: bool) -> Result<()> {
        crate::platform::chmod_paths(paths, mode, recursive)
    }

    #[cfg(not(target_os = "windows"))]
    fn chown_paths(&self, paths: &[PathBuf], owner_spec: &str, recursive: bool) -> Result<()> {
        crate::platform::chown_paths(paths, owner_spec, recursive)
    }
}
