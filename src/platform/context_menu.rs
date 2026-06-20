use std::path::PathBuf;

use anyhow::Result;

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct ContextMenuRequest {
    pub directory: PathBuf,
    pub selected_paths: Vec<PathBuf>,
}

#[allow(dead_code)]
pub fn show_context_menu(request: &ContextMenuRequest) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        crate::platform::windows::show_context_menu(request)?;
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        crate::platform::unix::show_context_menu(request)?;
        Ok(())
    }
}
