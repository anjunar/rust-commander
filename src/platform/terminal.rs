use std::path::Path;

use anyhow::Result;

pub fn open_console(path: &Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        crate::platform::windows::open_console(path)?;
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        crate::platform::unix::open_console(path)?;
        Ok(())
    }
}
