use std::path::PathBuf;

/// Whether clipboarded files should be pasted as a copy or a move (cut).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FileClipboardMode {
    #[default]
    Copy,
    Cut,
}

#[derive(Clone, Debug, Default)]
pub struct FileClipboard {
    mode: FileClipboardMode,
    paths: Vec<PathBuf>,
}

impl FileClipboard {
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    pub fn mode(&self) -> FileClipboardMode {
        self.mode
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    pub fn set(&mut self, mode: FileClipboardMode, paths: Vec<PathBuf>) {
        self.mode = mode;
        self.paths = paths;
    }

    pub fn clear(&mut self) {
        self.mode = FileClipboardMode::Copy;
        self.paths.clear();
    }

    /// Drop cut entries after a successful paste-move; leave copy clipboard intact.
    pub fn clear_if_cut(&mut self) {
        if self.mode == FileClipboardMode::Cut {
            self.clear();
        }
    }
}

/// Put local file paths on the OS clipboard for Explorer / other file managers.
pub fn set_system_file_clipboard(paths: &[PathBuf], mode: FileClipboardMode) -> bool {
    #[cfg(target_os = "windows")]
    {
        use crate::platform::file_clipboard::{set_files, WinFileClipboardMode};
        let win_mode = match mode {
            FileClipboardMode::Copy => WinFileClipboardMode::Copy,
            FileClipboardMode::Cut => WinFileClipboardMode::Cut,
        };
        set_files(paths, win_mode)
    }

    #[cfg(not(target_os = "windows"))]
    {
        use gtk::{gdk, gio, glib, prelude::*};
        let Some(display) = gdk::Display::default() else {
            return false;
        };
        if paths.is_empty() {
            return false;
        }
        let files: Vec<gio::File> = paths.iter().map(gio::File::for_path).collect();
        let file_list = gdk::FileList::from_array(&files);
        let provider = gdk::ContentProvider::for_value(&file_list.to_value());
        // Also advertise GNOME cut/copy marker used by Nautilus and others.
        let marker = match mode {
            FileClipboardMode::Copy => "copy",
            FileClipboardMode::Cut => "cut",
        };
        let mut gnome = String::from(marker);
        for path in paths {
            gnome.push('\n');
            gnome.push_str(&gio::File::for_path(path).uri());
        }
        let gnome_bytes = glib::Bytes::from(gnome.as_bytes());
        let gnome_provider =
            gdk::ContentProvider::for_bytes("x-special/gnome-copied-files", &gnome_bytes);
        let union = gdk::ContentProvider::new_union(&[provider, gnome_provider]);
        return display.clipboard().set_content(Some(&union)).is_ok();
    }
}

/// Read a file list from the OS clipboard (Explorer / other file managers).
pub fn read_system_file_clipboard() -> Option<(FileClipboardMode, Vec<PathBuf>)> {
    #[cfg(target_os = "windows")]
    {
        use crate::platform::file_clipboard::{get_files, WinFileClipboardMode};
        get_files().map(|(mode, paths)| {
            let mode = match mode {
                WinFileClipboardMode::Copy => FileClipboardMode::Copy,
                WinFileClipboardMode::Cut => FileClipboardMode::Cut,
            };
            (mode, paths)
        })
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Synchronous path is unavailable for GDK async reads; callers on Unix
        // use `read_system_file_clipboard_async` instead.
        None
    }
}

#[cfg(not(target_os = "windows"))]
pub fn read_system_file_clipboard_async<F>(on_done: F)
where
    F: FnOnce(Option<(FileClipboardMode, Vec<PathBuf>)>) + 'static,
{
    use gtk::{gdk, gio, glib, prelude::*};

    let Some(display) = gdk::Display::default() else {
        on_done(None);
        return;
    };
    let clipboard = display.clipboard();
    let formats = clipboard.formats();

    // Prefer GNOME marker (carries cut vs copy), then FileList.
    if formats.contain_mime_type("x-special/gnome-copied-files") {
        clipboard.read_async(
            &["x-special/gnome-copied-files"],
            glib::Priority::DEFAULT,
            gio::Cancellable::NONE,
            move |result| {
                let parsed = result.ok().and_then(|(stream, _mime)| {
                    use gio::prelude::InputStreamExt;
                    let bytes = stream.read_bytes(64 * 1024, gio::Cancellable::NONE).ok()?;
                    parse_gnome_copied_files(std::str::from_utf8(bytes.as_ref()).ok()?)
                });
                on_done(parsed);
            },
        );
        return;
    }

    if !formats.contains_type(gdk::FileList::static_type()) {
        on_done(None);
        return;
    }

    clipboard.read_value_async(
        gdk::FileList::static_type(),
        glib::Priority::DEFAULT,
        gio::Cancellable::NONE,
        move |result| {
            let paths = result.ok().and_then(|value| {
                let file_list = value.get::<gdk::FileList>().ok()?;
                let paths = file_list
                    .files()
                    .into_iter()
                    .filter_map(|file| file.path())
                    .filter(|path| path.exists())
                    .collect::<Vec<_>>();
                if paths.is_empty() {
                    None
                } else {
                    Some((FileClipboardMode::Copy, paths))
                }
            });
            on_done(paths);
        },
    );
}

#[cfg(not(target_os = "windows"))]
fn parse_gnome_copied_files(text: &str) -> Option<(FileClipboardMode, Vec<PathBuf>)> {
    use gtk::{gio, prelude::*};

    let mut lines = text.lines();
    let mode = match lines.next()? {
        "cut" => FileClipboardMode::Cut,
        "copy" => FileClipboardMode::Copy,
        _ => return None,
    };
    let paths = lines
        .filter_map(|uri| gio::File::for_uri(uri).path())
        .filter(|path| path.exists())
        .collect::<Vec<_>>();
    if paths.is_empty() {
        None
    } else {
        Some((mode, paths))
    }
}

pub fn clear_system_file_clipboard() {
    #[cfg(target_os = "windows")]
    {
        crate::platform::file_clipboard::clear();
    }

    #[cfg(not(target_os = "windows"))]
    {
        use gtk::gdk::prelude::*;
        if let Some(display) = gtk::gdk::Display::default() {
            let _ = display
                .clipboard()
                .set_content(None::<&gtk::gdk::ContentProvider>);
        }
    }
}

pub fn existing_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    paths.iter().filter(|path| path.exists()).cloned().collect()
}
