use std::{
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{anyhow, Context, Result};

use crate::platform::context_menu::ContextMenuRequest;

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
        // ShellExecuteW should return a value > 32 after asking Explorer to run
        // the default "open" verb for the selected path.
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

pub fn show_context_menu(request: &ContextMenuRequest) -> Result<()> {
    use anyhow::bail;
    use windows::Win32::System::Com::{
        CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
    };

    if !request.selected_paths.is_empty()
        && request
            .selected_paths
            .iter()
            .any(|path| !path.exists() || path.parent().is_none())
    {
        bail!("The selected item no longer exists");
    }

    if request.selected_paths.is_empty() && !request.directory.exists() {
        bail!("The current directory no longer exists");
    }

    unsafe {
        CoInitializeEx(
            None,
            COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE,
        )
        .ok()?;
    }

    let result = unsafe { show_context_menu_impl(request) };

    unsafe {
        CoUninitialize();
    }

    result
}

unsafe fn show_context_menu_impl(request: &ContextMenuRequest) -> Result<()> {
    use std::ptr::null_mut;

    use anyhow::Context;
    use windows::{
        core::{PCSTR, PCWSTR},
        Win32::{
            Foundation::{HWND, POINT},
            System::Com::CoTaskMemFree,
            UI::{
                Shell::{
                    IContextMenu, IShellFolder, SEE_MASK_UNICODE, CMINVOKECOMMANDINFOEX,
                    CMF_NORMAL, SHBindToParent, SHParseDisplayName,
                },
                WindowsAndMessaging::{
                    CreatePopupMenu, DestroyMenu, GetCursorPos, GetForegroundWindow,
                    PostMessageW, SetForegroundWindow, TPM_LEFTALIGN, TPM_RETURNCMD,
                    TPM_RIGHTBUTTON, TrackPopupMenu, WM_NULL,
                },
            },
        },
    };

    let menu = CreatePopupMenu().context("Could not create popup menu")?;
    let owner = GetForegroundWindow();

    let invoke_result = (|| -> Result<()> {
        let selection = if request.selected_paths.is_empty() {
            vec![request.directory.clone()]
        } else {
            request.selected_paths.clone()
        };

        let mut pidls = Vec::with_capacity(selection.len());
        let mut child_pidls = Vec::with_capacity(selection.len());
        let mut parent_folder: Option<IShellFolder> = None;

        for path in &selection {
            let wide = wide_null(path.as_os_str());
            let mut absolute_pidl = null_mut();
            SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut absolute_pidl, 0, None)
                .ok()
                .with_context(|| format!("Could not resolve {}", path.display()))?;

            let mut child_relative = null_mut();
            let bound_parent: IShellFolder = SHBindToParent(absolute_pidl, Some(&mut child_relative))
                .with_context(|| format!("Could not bind shell parent for {}", path.display()))?;

            if parent_folder.is_none() {
                parent_folder = Some(bound_parent);
            }

            pidls.push(absolute_pidl);
            child_pidls.push(child_relative);
        }

        let parent_folder = parent_folder.context("Could not resolve a shell folder")?;
        let child_pidls = child_pidls
            .iter()
            .map(|pidl| *pidl as *const _)
            .collect::<Vec<_>>();
        let context_menu: IContextMenu = parent_folder
            .GetUIObjectOf::<IContextMenu>(HWND::default(), child_pidls.as_slice(), None)
            .context("Could not create the native context menu")?;

        context_menu
            .QueryContextMenu(menu, 0, 1, 0x7FFF, CMF_NORMAL)
            .ok()
            .context("Could not populate the native context menu")?;

        let mut cursor = POINT::default();
        GetCursorPos(&mut cursor)
            .ok()
            .context("Could not read the current cursor position")?;

        let _ = SetForegroundWindow(owner);
        let command_id = TrackPopupMenu(
            menu,
            TPM_LEFTALIGN | TPM_RETURNCMD | TPM_RIGHTBUTTON,
            cursor.x,
            cursor.y,
            Some(0),
            owner,
            None,
        )
        .0;
        let _ = PostMessageW(Some(owner), WM_NULL, Default::default(), Default::default());

        if command_id == 0 {
            return Ok(());
        }

        let mut invoke = CMINVOKECOMMANDINFOEX::default();
        invoke.cbSize = std::mem::size_of::<CMINVOKECOMMANDINFOEX>() as u32;
        invoke.fMask = SEE_MASK_UNICODE;
        invoke.hwnd = owner;
        invoke.lpVerb = PCSTR((command_id - 1) as usize as *const u8);
        invoke.lpVerbW = PCWSTR((command_id - 1) as usize as *const u16);
        invoke.nShow = windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        context_menu
            .InvokeCommand((&invoke as *const CMINVOKECOMMANDINFOEX).cast())
            .ok()
            .context("Could not execute the selected Explorer command")?;

        for pidl in pidls {
            CoTaskMemFree(Some(pidl.cast()));
        }

        Ok(())
    })();

    let _ = DestroyMenu(menu);
    invoke_result
}

fn wide_null(value: &std::ffi::OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

fn to_wide(value: &std::ffi::OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

fn null_mut_hwnd() -> *mut core::ffi::c_void {
    std::ptr::null::<core::ffi::c_void>() as *mut core::ffi::c_void
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::to_wide;

    #[test]
    fn to_wide_appends_trailing_nul() {
        let wide = to_wide(OsStr::new("open"));
        assert_eq!(wide.last().copied(), Some(0));
        assert_eq!(
            wide[..wide.len() - 1],
            ['o' as u16, 'p' as u16, 'e' as u16, 'n' as u16]
        );
    }
}
