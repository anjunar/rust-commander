use std::{
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, anyhow};

use crate::domain::roots::RootLocation;

pub fn available_roots() -> Vec<RootLocation> {
    let mut roots = Vec::new();

    if let Some(home) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
        roots.push(RootLocation {
            label: "Home".into(),
            path: home,
        });
    }

    for letter in b'A'..=b'Z' {
        let drive = format!("{}:\\", letter as char);
        let path = PathBuf::from(&drive);
        if path.exists() && !roots.iter().any(|root| root.path == path) {
            roots.push(RootLocation {
                label: format!("{}:", letter as char),
                path,
            });
        }
    }

    roots
}

pub fn open_path(path: &Path) -> Result<()> {
    use std::{ffi::OsStr, ptr::null};

    use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};

    let operation = to_wide(OsStr::new("open"));
    let target = to_wide(path.as_os_str());

    let result = unsafe {
        ShellExecuteW(
            null_mut_hwnd(),
            operation.as_ptr(),
            target.as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        )
    };

    if result as usize <= 32 {
        return Err(anyhow!("ShellExecuteW failed")).with_context(|| {
            format!("Could not launch the default action for {}", path.display())
        });
    }

    Ok(())
}

pub fn open_console(path: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;

    use windows_sys::Win32::System::Threading::CREATE_NEW_CONSOLE;

    Command::new("cmd.exe")
        .arg("/K")
        .current_dir(path)
        .creation_flags(CREATE_NEW_CONSOLE)
        .spawn()
        .with_context(|| format!("Could not open a console for {}", path.display()))?;

    Ok(())
}

fn to_wide(value: &std::ffi::OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

fn null_mut_hwnd() -> *mut core::ffi::c_void {
    std::ptr::null::<core::ffi::c_void>() as *mut core::ffi::c_void
}
