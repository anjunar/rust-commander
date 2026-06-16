use std::path::Path;

use anyhow::Result;

pub fn open_path(path: &Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        crate::platform::windows::open_path(path)?;
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        crate::platform::unix::open_path(path)?;
        Ok(())
    }
}
