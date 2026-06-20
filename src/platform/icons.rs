use std::{
    cell::RefCell,
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use gtk::gdk;

use crate::domain::{entry::Entry, panel_location::PanelLocation};

#[derive(Clone)]
pub struct FileIcon {
    pub paintable: Option<gdk::Paintable>,
    pub icon_name: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PathStamp {
    modified: Option<SystemTime>,
    len: u64,
}

#[derive(Clone)]
struct CachedIcon {
    icon: FileIcon,
    stamp: Option<PathStamp>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum IconKey {
    ParentDirectory,
    ExistingPath(PathBuf),
    FileExtension(String),
    Directory,
    File,
}

pub fn icon_for_entry(location: &PanelLocation, entry: &Entry) -> FileIcon {
    let key = icon_key_for_entry(location, entry);
    let stamp = cache_stamp_for_key(&key);

    ICON_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(cached) = cache.get(&key) {
            if cached.stamp == stamp {
                return cached.icon.clone();
            }
        }

        let icon = FileIcon {
            paintable: load_icon(&key),
            icon_name: fallback_icon_name(entry),
        };
        cache.insert(
            key,
            CachedIcon {
                icon: icon.clone(),
                stamp,
            },
        );
        icon
    })
}

fn icon_key_for_entry(location: &PanelLocation, entry: &Entry) -> IconKey {
    if entry.is_parent_link {
        return IconKey::ParentDirectory;
    }

    if let Some(base_path) = location.filesystem_path() {
        let full_path = base_path.join(&entry.name);
        if full_path.exists() {
            return IconKey::ExistingPath(full_path);
        }
    }

    if entry.is_dir {
        IconKey::Directory
    } else if let Some(extension) = Path::new(&entry.name)
        .extension()
        .and_then(|ext| ext.to_str())
    {
        IconKey::FileExtension(extension.to_ascii_lowercase())
    } else {
        IconKey::File
    }
}

fn fallback_icon_name(entry: &Entry) -> &'static str {
    if entry.is_parent_link {
        "go-up-symbolic"
    } else if entry.is_dir {
        "folder-symbolic"
    } else {
        "text-x-generic-symbolic"
    }
}

#[cfg(target_os = "windows")]
fn load_icon(key: &IconKey) -> Option<gdk::Paintable> {
    windows::load_icon(key)
}

#[cfg(not(target_os = "windows"))]
fn load_icon(_key: &IconKey) -> Option<gdk::Paintable> {
    None
}

thread_local! {
    static ICON_CACHE: RefCell<HashMap<IconKey, CachedIcon>> = RefCell::new(HashMap::new());
}

fn cache_stamp_for_key(key: &IconKey) -> Option<PathStamp> {
    match key {
        IconKey::ExistingPath(path) => path_stamp(path),
        _ => None,
    }
}

fn path_stamp(path: &Path) -> Option<PathStamp> {
    let metadata = fs::metadata(path).ok()?;
    Some(PathStamp {
        modified: metadata.modified().ok(),
        len: metadata.len(),
    })
}

#[cfg(target_os = "windows")]
mod windows {
    use std::{ffi::OsStr, os::windows::ffi::OsStrExt, path::Path, ptr::null_mut, slice};

    use gtk::gdk::{self, MemoryFormat, MemoryTexture};
    use gtk::glib::{object::Cast, Bytes};
    use windows_sys::Win32::{
        Graphics::Gdi::{
            CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
            SelectObject, BITMAPV5HEADER, BI_BITFIELDS, DIB_RGB_COLORS,
        },
        Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL},
        UI::{
            Shell::{
                SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_SMALLICON, SHGFI_USEFILEATTRIBUTES,
            },
            WindowsAndMessaging::{DestroyIcon, DrawIconEx, DI_NORMAL},
        },
    };

    use super::IconKey;

    const ICON_SIZE: i32 = 20;

    pub fn load_icon(key: &IconKey) -> Option<gdk::Paintable> {
        let (path, attributes, use_file_attributes) = icon_request(key);
        let path_wide = to_wide(path.as_os_str());

        let mut file_info = SHFILEINFOW::default();
        let flags = SHGFI_ICON
            | SHGFI_SMALLICON
            | if use_file_attributes {
                SHGFI_USEFILEATTRIBUTES
            } else {
                0
            };

        let result = unsafe {
            // Ask the shell for the icon associated with either the concrete path
            // or a synthetic extension/directory hint when no real file is needed.
            SHGetFileInfoW(
                path_wide.as_ptr(),
                attributes,
                &mut file_info,
                std::mem::size_of::<SHFILEINFOW>() as u32,
                flags,
            )
        };

        if result == 0 || file_info.hIcon.is_null() {
            return None;
        }

        let paintable = unsafe { icon_handle_to_paintable(file_info.hIcon) };
        unsafe {
            DestroyIcon(file_info.hIcon);
        }
        paintable
    }

    fn icon_request(key: &IconKey) -> (&Path, u32, bool) {
        match key {
            IconKey::ParentDirectory | IconKey::Directory => {
                (Path::new("folder"), FILE_ATTRIBUTE_DIRECTORY, true)
            }
            IconKey::ExistingPath(path) => (path.as_path(), 0, false),
            IconKey::FileExtension(extension) => {
                (Path::new(extension), FILE_ATTRIBUTE_NORMAL, true)
            }
            IconKey::File => (Path::new("file"), FILE_ATTRIBUTE_NORMAL, true),
        }
    }

    unsafe fn icon_handle_to_paintable(icon: *mut core::ffi::c_void) -> Option<gdk::Paintable> {
        let screen_dc = unsafe { GetDC(0 as _) };
        if screen_dc.is_null() {
            return None;
        }

        let memory_dc = unsafe { CreateCompatibleDC(screen_dc) };
        if memory_dc.is_null() {
            unsafe {
                ReleaseDC(0 as _, screen_dc);
            }
            return None;
        }

        let header = BITMAPV5HEADER {
            bV5Size: std::mem::size_of::<BITMAPV5HEADER>() as u32,
            bV5Width: ICON_SIZE,
            bV5Height: -ICON_SIZE,
            bV5Planes: 1,
            bV5BitCount: 32,
            bV5Compression: BI_BITFIELDS,
            bV5RedMask: 0x00FF_0000,
            bV5GreenMask: 0x0000_FF00,
            bV5BlueMask: 0x0000_00FF,
            bV5AlphaMask: 0xFF00_0000,
            ..unsafe { std::mem::zeroed() }
        };

        let mut bits = null_mut();
        let bitmap = unsafe {
            // Render the native HICON into a 32-bit DIB so GTK can wrap the BGRA
            // pixels in a MemoryTexture without any Win32-specific lifetime hooks.
            CreateDIBSection(
                memory_dc,
                &header as *const _ as *const _,
                DIB_RGB_COLORS,
                &mut bits,
                null_mut(),
                0,
            )
        };

        if bitmap.is_null() || bits.is_null() {
            unsafe {
                DeleteDC(memory_dc);
                ReleaseDC(0 as _, screen_dc);
            }
            return None;
        }

        let previous = unsafe { SelectObject(memory_dc, bitmap as _) };
        unsafe {
            std::ptr::write_bytes(bits, 0, (ICON_SIZE * ICON_SIZE * 4) as usize);
        }

        let draw_ok = unsafe {
            DrawIconEx(
                memory_dc,
                0,
                0,
                icon,
                ICON_SIZE,
                ICON_SIZE,
                0,
                null_mut(),
                DI_NORMAL,
            ) != 0
        };

        let paintable = if draw_ok {
            let src = unsafe {
                slice::from_raw_parts(bits as *const u8, (ICON_SIZE * ICON_SIZE * 4) as usize)
            };
            let bytes = Bytes::from_owned(src.to_vec());
            let texture = MemoryTexture::new(
                ICON_SIZE,
                ICON_SIZE,
                MemoryFormat::B8g8r8a8,
                &bytes,
                (ICON_SIZE * 4) as usize,
            );
            Some(texture.upcast::<gdk::Paintable>())
        } else {
            None
        };

        unsafe {
            SelectObject(memory_dc, previous);
            DeleteObject(bitmap as _);
            DeleteDC(memory_dc);
            ReleaseDC(0 as _, screen_dc);
        }

        paintable
    }

    fn to_wide(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain(std::iter::once(0)).collect()
    }

    #[cfg(test)]
    mod tests {
        include!("../../tests/unit/platform_icons_windows_tests.rs");
    }
}

#[cfg(test)]
#[path = "../../tests/unit/platform_icons_tests.rs"]
mod tests;
