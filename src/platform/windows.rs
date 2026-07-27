use std::{
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result};
use gtk::glib::object::IsA;

use crate::domain::roots::RootLocation;
use crate::platform::assets::asset_path;
use crate::platform::context_menu::ContextMenuRequest;

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
    use std::os::windows::process::CommandExt;

    use windows_sys::Win32::System::Threading::{
        CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, DETACHED_PROCESS,
    };

    // Out-of-process associations: in-process ShellExecute can load hostile
    // shell extensions into the UI process.
    let absolute = normalize_open_path(path);

    Command::new("cmd.exe")
        .args(["/C", "start", "", "/B"])
        .arg(&absolute)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .with_context(|| format!("Could not launch the default action for {}", path.display()))?;

    Ok(())
}

fn normalize_open_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

/// Open the native Windows Explorer Properties sheet for a local path.
pub fn show_file_properties(path: &Path) -> Result<()> {
    use windows_sys::Win32::UI::Shell::{SHObjectProperties, SHOP_FILEPATH};

    let absolute = normalize_open_path(path);
    if !absolute.exists() {
        anyhow::bail!("Path does not exist: {}", absolute.display());
    }

    let file = wide_null(absolute.as_os_str());
    let ok = unsafe {
        SHObjectProperties(
            std::ptr::null_mut(),
            SHOP_FILEPATH as u32,
            file.as_ptr(),
            std::ptr::null(),
        )
    };
    if ok == 0 {
        anyhow::bail!(
            "Could not open Windows properties for {}",
            absolute.display()
        );
    }
    Ok(())
}

pub fn open_console(path: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;

    use windows_sys::Win32::System::Threading::CREATE_NEW_CONSOLE;

    Command::new("powershell.exe")
        .arg("-NoExit")
        .current_dir(path)
        .creation_flags(CREATE_NEW_CONSOLE)
        .spawn()
        .with_context(|| format!("Could not open a console for {}", path.display()))?;

    Ok(())
}

/// Force the window with `window_title` to the foreground.
///
/// Needed after closing a modal on Windows: if the user Alt-Tabbed away while the
/// dialog was open, destroying the dialog often activates that other app, and a
/// later GTK `present()` cannot steal focus under the foreground lock.
pub fn force_foreground_window(window_title: &str) {
    let hwnd = find_hwnd_by_title(window_title);
    force_foreground_hwnd(hwnd);
}

/// Activate a GTK window using its native Win32 HWND (preferred over title lookup).
pub fn force_foreground_gtk_window(window: &impl IsA<gtk::Window>) {
    use gdk4_win32::Win32Surface;
    use gtk::prelude::{Cast, NativeExt};

    let window = window.upcast_ref::<gtk::Window>();
    let Some(surface) = window.surface() else {
        return;
    };
    let Ok(win32) = surface.downcast::<Win32Surface>() else {
        return;
    };
    force_foreground_hwnd(win32.handle().0 as _);
}

fn force_foreground_hwnd(hwnd: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::{
        System::Threading::{AttachThreadInput, GetCurrentThreadId},
        UI::WindowsAndMessaging::{
            BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, IsIconic,
            SetForegroundWindow, SetWindowPos, ShowWindow, HWND_NOTOPMOST, HWND_TOPMOST,
            SWP_NOMOVE, SWP_NOSIZE, SW_RESTORE,
        },
    };

    if hwnd.is_null() {
        return;
    }

    unsafe {
        if IsIconic(hwnd) != 0 {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }

        let foreground = GetForegroundWindow();
        if foreground == hwnd {
            return;
        }

        let fore_thread = GetWindowThreadProcessId(foreground, std::ptr::null_mut());
        let cur_thread = GetCurrentThreadId();
        if fore_thread != 0 && fore_thread != cur_thread {
            let _ = AttachThreadInput(cur_thread, fore_thread, 1);
            let _ = SetForegroundWindow(hwnd);
            let _ = BringWindowToTop(hwnd);
            let _ = AttachThreadInput(cur_thread, fore_thread, 0);
        } else {
            let _ = SetForegroundWindow(hwnd);
            let _ = BringWindowToTop(hwnd);
        }

        // If SetForegroundWindow is blocked, briefly toggle TOPMOST to raise the window.
        if GetForegroundWindow() != hwnd {
            let _ = SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
            let _ = SetWindowPos(hwnd, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

fn find_hwnd_by_title(window_title: &str) -> windows_sys::Win32::Foundation::HWND {
    use std::ffi::OsStr;

    use windows_sys::Win32::{
        Foundation::HWND,
        System::Threading::GetCurrentProcessId,
        UI::WindowsAndMessaging::{
            EnumWindows, FindWindowW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
        },
    };

    let title_w = to_wide(OsStr::new(window_title));
    let hwnd = unsafe { FindWindowW(std::ptr::null(), title_w.as_ptr()) };
    if !hwnd.is_null() {
        return hwnd;
    }

    struct EnumState {
        pid: u32,
        title: String,
        found: HWND,
    }

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: isize) -> i32 {
        let state = &mut *(lparam as *mut EnumState);
        if IsWindowVisible(hwnd) == 0 {
            return 1;
        }

        let mut pid = 0_u32;
        let _ = GetWindowThreadProcessId(hwnd, &mut pid);
        if pid != state.pid {
            return 1;
        }

        let mut buf = [0_u16; 512];
        let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        if len <= 0 {
            return 1;
        }
        let title = String::from_utf16_lossy(&buf[..len as usize]);
        if title == state.title
            || title.starts_with(&state.title)
            || state.title.starts_with(&title)
        {
            state.found = hwnd;
            return 0;
        }
        1
    }

    let mut state = EnumState {
        pid: unsafe { GetCurrentProcessId() },
        title: window_title.to_string(),
        found: std::ptr::null_mut(),
    };
    unsafe {
        let _ = EnumWindows(Some(enum_proc), &mut state as *mut _ as isize);
    }
    state.found
}

pub fn apply_runtime_window_icon(window_title: &str) -> Result<()> {
    use std::ffi::OsStr;

    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, LoadImageW, SendMessageW, ICON_BIG, ICON_SMALL, IMAGE_ICON, LR_LOADFROMFILE,
        WM_SETICON,
    };

    let title_w = to_wide(OsStr::new(window_title));
    let hwnd = unsafe { FindWindowW(std::ptr::null(), title_w.as_ptr()) };
    if hwnd.is_null() {
        return Ok(());
    }

    let ico_path = asset_path("assets/icons/app_icon.ico");
    if !ico_path.exists() {
        return Ok(());
    }

    let wide = to_wide(ico_path.as_os_str());
    let hicon = unsafe {
        LoadImageW(
            std::ptr::null_mut(),
            wide.as_ptr(),
            IMAGE_ICON,
            0,
            0,
            LR_LOADFROMFILE,
        )
    };
    if hicon.is_null() {
        return Ok(());
    }

    unsafe {
        let _ = SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, hicon as isize);
        let _ = SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, hicon as isize);
    }

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
        CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).ok()?;
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
                    IContextMenu, IShellFolder, SHBindToParent, SHParseDisplayName, CMF_NORMAL,
                    CMINVOKECOMMANDINFOEX, SEE_MASK_UNICODE,
                },
                WindowsAndMessaging::{
                    CreatePopupMenu, DestroyMenu, GetCursorPos, GetForegroundWindow, PostMessageW,
                    SetForegroundWindow, TrackPopupMenu, TPM_LEFTALIGN, TPM_RETURNCMD,
                    TPM_RIGHTBUTTON, WM_NULL,
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
            let bound_parent: IShellFolder =
                SHBindToParent(absolute_pidl, Some(&mut child_relative)).with_context(|| {
                    format!("Could not bind shell parent for {}", path.display())
                })?;

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

        let invoke = CMINVOKECOMMANDINFOEX {
            cbSize: std::mem::size_of::<CMINVOKECOMMANDINFOEX>() as u32,
            fMask: SEE_MASK_UNICODE,
            hwnd: owner,
            lpVerb: PCSTR((command_id - 1) as usize as *const u8),
            lpVerbW: PCWSTR((command_id - 1) as usize as *const u16),
            nShow: windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
            ..Default::default()
        };

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

/// Resolves a Windows `.lnk` shortcut to its target path, if possible.
pub fn resolve_shortcut_target(path: &Path) -> Option<PathBuf> {
    use windows::{
        core::{Interface, PCWSTR},
        Win32::{
            Foundation::MAX_PATH,
            System::Com::{
                CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile,
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, STGM_READ,
            },
            UI::Shell::{IShellLinkW, ShellLink},
        },
    };

    if !path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lnk"))
    {
        return None;
    }

    unsafe {
        let com_initialized =
            CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).is_ok();

        let result = (|| -> Option<PathBuf> {
            let shell_link: IShellLinkW =
                CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
            let persist: IPersistFile = shell_link.cast().ok()?;
            let wide = to_wide(path.as_os_str());
            persist.Load(PCWSTR(wide.as_ptr()), STGM_READ).ok()?;

            let mut buffer = vec![0u16; MAX_PATH as usize];
            shell_link
                .GetPath(&mut buffer, std::ptr::null_mut(), 0)
                .ok()?;
            let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
            if len == 0 {
                return None;
            }
            let target = String::from_utf16_lossy(&buffer[..len]);
            if target.is_empty() {
                None
            } else {
                Some(PathBuf::from(target))
            }
        })();

        if com_initialized {
            CoUninitialize();
        }

        result
    }
}

#[cfg(test)]
#[path = "../../tests/unit/platform_windows_tests.rs"]
mod tests;
