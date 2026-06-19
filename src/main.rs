#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use anyhow::Result;

fn main() -> Result<()> {
    rcommander::run()
}
