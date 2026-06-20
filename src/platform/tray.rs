#[cfg(target_os = "windows")]
mod windows_tray {
    use std::{ffi::OsStr, os::windows::ffi::OsStrExt};
    use windows_sys::Win32::UI::Shell::{
        Shell_NotifyIconW, NIF_ICON, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{LoadImageW, IMAGE_ICON, LR_LOADFROMFILE};

    use crate::platform::assets::asset_path;

    pub fn create_tray_icon() -> Result<(), String> {
        let ico_path = asset_path("assets/icons/app_icon.ico");
        if !ico_path.exists() {
            return Err(format!("ICO not found: {}", ico_path.display()));
        }

        let path_str = ico_path.to_str().ok_or("Invalid path string")?;
        let wide: Vec<u16> = OsStr::new(path_str)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            // hinst = null, name = path, returns HICON as isize
            let hicon = LoadImageW(
                std::ptr::null_mut(),
                wide.as_ptr(),
                IMAGE_ICON,
                0,
                0,
                LR_LOADFROMFILE,
            ) as *mut core::ffi::c_void;
            if hicon.is_null() {
                return Err("LoadImageW failed".into());
            }

            let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hIcon = hicon;
            nid.uFlags = NIF_ICON | NIF_TIP;
            copy_tip(&mut nid.szTip, "RCommander");

            // Adding the icon should register it with the shell notification area;
            // a zero return means the tray entry never became visible.
            let res = Shell_NotifyIconW(NIM_ADD, &mut nid as *mut NOTIFYICONDATAW);
            if res == 0 {
                return Err("Shell_NotifyIconW(NIM_ADD) failed".into());
            }
        }

        Ok(())
    }

    pub fn remove_tray_icon() {
        unsafe {
            let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            // Best-effort cleanup of the tray slot that was created in create_tray_icon.
            let _ = Shell_NotifyIconW(NIM_DELETE, &mut nid as *mut NOTIFYICONDATAW);
        }
    }

    fn copy_tip(target: &mut [u16], tip: &str) {
        let tip_w: Vec<u16> = OsStr::new(tip)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        for (slot, value) in target.iter_mut().zip(tip_w.into_iter()) {
            *slot = value;
        }
    }

    #[cfg(test)]
    mod tests {
        use super::copy_tip;

        #[test]
        fn copy_tip_writes_nul_terminated_utf16() {
            let mut buffer = [0u16; 8];
            copy_tip(&mut buffer, "RC");
            assert_eq!(&buffer[..3], &[b'R' as u16, b'C' as u16, 0]);
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod windows_tray {}

#[cfg(target_os = "windows")]
pub use windows_tray::{create_tray_icon, remove_tray_icon};

#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
pub fn create_tray_icon() -> Result<(), String> {
    Ok(())
}

#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
pub fn remove_tray_icon() {}
