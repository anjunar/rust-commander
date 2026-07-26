//! Native Windows file clipboard (CF_HDROP) for Explorer interop.

use std::{
    ffi::OsStr,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};

use windows_sys::Win32::{
    Foundation::{GlobalFree, HANDLE, POINT},
    System::{
        DataExchange::{
            CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable,
            OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
        },
        Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GHND},
        Ole::{CF_HDROP, DROPEFFECT_COPY, DROPEFFECT_MOVE},
    },
    UI::Shell::{DragQueryFileW, CFSTR_PREFERREDDROPEFFECT, DROPFILES, HDROP},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WinFileClipboardMode {
    Copy,
    Cut,
}

pub fn set_files(paths: &[PathBuf], mode: WinFileClipboardMode) -> bool {
    if paths.is_empty() {
        return false;
    }

    let mut file_list: Vec<u16> = Vec::new();
    for path in paths {
        let absolute = normalize_path(path);
        file_list.extend(OsStr::new(&absolute).encode_wide());
        file_list.push(0);
    }
    file_list.push(0); // double-null terminator

    let dropfiles_size = std::mem::size_of::<DROPFILES>();
    let bytes = dropfiles_size + file_list.len() * 2;
    let hglobal = unsafe { GlobalAlloc(GHND, bytes) };
    if hglobal.is_null() {
        return false;
    }

    unsafe {
        let ptr = GlobalLock(hglobal) as *mut u8;
        if ptr.is_null() {
            let _ = GlobalFree(hglobal);
            return false;
        }

        let dropfiles = DROPFILES {
            pFiles: dropfiles_size as u32,
            pt: POINT { x: 0, y: 0 },
            fNC: 0,
            fWide: 1,
        };
        std::ptr::copy_nonoverlapping(
            (&dropfiles as *const DROPFILES).cast::<u8>(),
            ptr,
            dropfiles_size,
        );
        std::ptr::copy_nonoverlapping(
            file_list.as_ptr().cast::<u8>(),
            ptr.add(dropfiles_size),
            file_list.len() * 2,
        );
        GlobalUnlock(hglobal);

        if OpenClipboard(std::ptr::null_mut()) == 0 {
            let _ = GlobalFree(hglobal);
            return false;
        }

        EmptyClipboard();
        if SetClipboardData(CF_HDROP as u32, hglobal as HANDLE).is_null() {
            CloseClipboard();
            let _ = GlobalFree(hglobal);
            return false;
        }

        // Ownership of hglobal transferred to the clipboard on success.
        let effect = match mode {
            WinFileClipboardMode::Copy => DROPEFFECT_COPY,
            WinFileClipboardMode::Cut => DROPEFFECT_MOVE,
        };
        set_preferred_drop_effect(effect);
        CloseClipboard();
    }

    true
}

pub fn get_files() -> Option<(WinFileClipboardMode, Vec<PathBuf>)> {
    unsafe {
        if IsClipboardFormatAvailable(CF_HDROP as u32) == 0 {
            return None;
        }
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return None;
        }

        let handle = GetClipboardData(CF_HDROP as u32);
        if handle.is_null() {
            CloseClipboard();
            return None;
        }

        let hdrop = handle as HDROP;
        let count = DragQueryFileW(hdrop, 0xFFFF_FFFF, std::ptr::null_mut(), 0);
        if count == 0 {
            CloseClipboard();
            return None;
        }

        let mut paths = Vec::with_capacity(count as usize);
        for index in 0..count {
            let len = DragQueryFileW(hdrop, index, std::ptr::null_mut(), 0);
            if len == 0 {
                continue;
            }
            let mut buffer = vec![0u16; (len as usize) + 1];
            let written = DragQueryFileW(hdrop, index, buffer.as_mut_ptr(), buffer.len() as u32);
            if written == 0 {
                continue;
            }
            buffer.truncate(written as usize);
            let path = PathBuf::from(std::ffi::OsString::from_wide(&buffer));
            if path.exists() {
                paths.push(path);
            }
        }

        let mode = match preferred_drop_effect() {
            Some(effect) if effect & DROPEFFECT_MOVE != 0 => WinFileClipboardMode::Cut,
            _ => WinFileClipboardMode::Copy,
        };

        CloseClipboard();

        if paths.is_empty() {
            None
        } else {
            Some((mode, paths))
        }
    }
}

pub fn clear() {
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) != 0 {
            EmptyClipboard();
            CloseClipboard();
        }
    }
}

unsafe fn set_preferred_drop_effect(effect: u32) {
    let format = RegisterClipboardFormatW(CFSTR_PREFERREDDROPEFFECT);
    if format == 0 {
        return;
    }
    let hglobal = GlobalAlloc(GHND, std::mem::size_of::<u32>());
    if hglobal.is_null() {
        return;
    }
    let ptr = GlobalLock(hglobal) as *mut u32;
    if ptr.is_null() {
        let _ = GlobalFree(hglobal);
        return;
    }
    *ptr = effect;
    GlobalUnlock(hglobal);
    if SetClipboardData(format, hglobal as HANDLE).is_null() {
        let _ = GlobalFree(hglobal);
    }
}

unsafe fn preferred_drop_effect() -> Option<u32> {
    let format = RegisterClipboardFormatW(CFSTR_PREFERREDDROPEFFECT);
    if format == 0 || IsClipboardFormatAvailable(format) == 0 {
        return None;
    }
    let handle = GetClipboardData(format);
    if handle.is_null() {
        return None;
    }
    let ptr = GlobalLock(handle as _) as *const u32;
    if ptr.is_null() {
        return None;
    }
    let effect = *ptr;
    GlobalUnlock(handle as _);
    Some(effect)
}

fn normalize_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}
