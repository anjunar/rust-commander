use std::path::{Path, PathBuf};

pub fn asset_path(relative: impl AsRef<Path>) -> PathBuf {
    let relative = relative.as_ref();

    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            for ancestor in exe_dir.ancestors() {
                let candidate = ancestor.join(relative);
                if candidate.exists() {
                    return candidate;
                }
            }
        }
    }

    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}
