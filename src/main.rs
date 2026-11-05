#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use anyhow::Result;

fn main() -> Result<()> {
    if let Some(code) = rcommander::archive_worker_main() {
        std::process::exit(code);
    }
    rcommander::run()
}
