#[cfg(target_os = "windows")]
mod imp {
    use std::{ffi::OsStr, os::windows::ffi::OsStrExt};

    use windows_sys::Win32::{
        Foundation::RECT,
        UI::WindowsAndMessaging::{
            FindWindowW, GetWindowPlacement, SW_SHOWMAXIMIZED, SW_SHOWNORMAL, SetWindowPlacement,
            WINDOWPLACEMENT,
        },
    };

    #[derive(Clone, Copy, Debug)]
    pub struct WindowPlacementState {
        pub x: i32,
        pub y: i32,
        pub width: i32,
        pub height: i32,
        pub maximized: bool,
    }

    pub fn current_window_placement(title: &str) -> Option<WindowPlacementState> {
        let hwnd = find_window(title)?;
        let mut placement = WINDOWPLACEMENT {
            length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
            ..unsafe { std::mem::zeroed() }
        };
        let ok = unsafe { GetWindowPlacement(hwnd, &mut placement) };
        if ok == 0 {
            return None;
        }

        let rect = placement.rcNormalPosition;
        Some(WindowPlacementState {
            x: rect.left,
            y: rect.top,
            width: rect.right - rect.left,
            height: rect.bottom - rect.top,
            maximized: placement.showCmd == SW_SHOWMAXIMIZED as u32,
        })
    }

    pub fn restore_window_placement(
        title: &str,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        maximized: bool,
    ) {
        let Some(hwnd) = find_window(title) else {
            return;
        };

        let mut placement = WINDOWPLACEMENT {
            length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
            ..unsafe { std::mem::zeroed() }
        };
        let ok = unsafe { GetWindowPlacement(hwnd, &mut placement) };
        if ok == 0 {
            return;
        }

        placement.showCmd = if maximized {
            SW_SHOWMAXIMIZED as u32
        } else {
            SW_SHOWNORMAL as u32
        };
        placement.rcNormalPosition = RECT {
            left: x,
            top: y,
            right: x + width,
            bottom: y + height,
        };

        unsafe {
            let _ = SetWindowPlacement(hwnd, &placement);
        }
    }

    fn find_window(title: &str) -> Option<*mut core::ffi::c_void> {
        let title_wide: Vec<u16> = OsStr::new(title)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let hwnd = unsafe { FindWindowW(std::ptr::null(), title_wide.as_ptr()) };
        (!hwnd.is_null()).then_some(hwnd)
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    #[derive(Clone, Copy, Debug)]
    pub struct WindowPlacementState {
        pub x: i32,
        pub y: i32,
        pub width: i32,
        pub height: i32,
        pub maximized: bool,
    }

    pub fn current_window_placement(_title: &str) -> Option<WindowPlacementState> {
        None
    }

    pub fn restore_window_placement(
        _title: &str,
        _x: i32,
        _y: i32,
        _width: i32,
        _height: i32,
        _maximized: bool,
    ) {
    }
}

pub use imp::{WindowPlacementState, current_window_placement, restore_window_placement};
