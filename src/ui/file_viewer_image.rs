use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    sync::OnceLock,
    thread,
};

use anyhow::{bail, Context, Result};
use gtk::{
    gdk::{self, MemoryFormat, MemoryTexture},
    glib::{self, Bytes},
    prelude::*,
};
use image::GenericImageView;
use rust_i18n::t;

use crate::{
    domain::panel::Panel,
    fs::reader::format_bytes,
    ui::file_viewer_common::{
        build_viewer_chrome, file_size, present_with_focus, set_viewer_path, ViewerChrome,
    },
};

pub(crate) fn is_image_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "tif" | "tiff" | "svg")
    )
}

fn is_svg_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("svg"))
}

/// Image paths from the panel in its current sort order.
pub(crate) fn sibling_images_from_panel(panel: &Panel) -> Vec<PathBuf> {
    panel
        .entries
        .iter()
        .filter(|entry| !entry.is_dir && !entry.is_parent_link)
        .filter_map(|entry| panel.location.entry_filesystem_path(entry))
        .filter(|path| is_image_path(path))
        .collect()
}

struct DecodedPixels {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

struct LoadedImage {
    texture: MemoryTexture,
    width: u32,
    height: u32,
}

struct ImageViewerState {
    paths: Vec<PathBuf>,
    index: usize,
    load_gen: u64,
    chrome: ViewerChrome,
    stack: gtk::Stack,
    picture: gtk::Picture,
    status_label: gtk::Label,
    dims_label: gtk::Label,
}

pub(crate) fn open(
    parent: &gtk::ApplicationWindow,
    path: &Path,
    sibling_images: Vec<PathBuf>,
) -> Result<()> {
    let mut paths = sibling_images
        .into_iter()
        .filter(|candidate| is_image_path(candidate))
        .collect::<Vec<_>>();
    if paths.is_empty() {
        paths.push(path.to_path_buf());
    }

    let index = paths
        .iter()
        .position(|candidate| candidate == path)
        .unwrap_or(0);
    if !paths.iter().any(|candidate| candidate == path) {
        paths.insert(index, path.to_path_buf());
    }

    let chrome = build_viewer_chrome(parent, &paths[index]);

    let spinner = gtk::Spinner::new();
    spinner.set_spinning(true);
    spinner.set_size_request(48, 48);
    let loading_label = gtk::Label::new(Some(&t!("viewer.image_loading")));
    loading_label.add_css_class("editor-status");
    let loading_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
    loading_box.set_halign(gtk::Align::Center);
    loading_box.set_valign(gtk::Align::Center);
    loading_box.set_hexpand(true);
    loading_box.set_vexpand(true);
    loading_box.append(&spinner);
    loading_box.append(&loading_label);

    let picture = gtk::Picture::new();
    picture.set_can_shrink(true);
    picture.set_content_fit(gtk::ContentFit::Contain);
    picture.set_hexpand(true);
    picture.set_vexpand(true);

    let scrolled = gtk::ScrolledWindow::builder()
        .child(&picture)
        .hexpand(true)
        .vexpand(true)
        .build();
    scrolled.add_css_class("panel-scroller");

    let stack = gtk::Stack::new();
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    stack.add_named(&loading_box, Some("loading"));
    stack.add_named(&scrolled, Some("image"));
    stack.set_visible_child_name("loading");
    chrome.content.append(&stack);

    let status_label = gtk::Label::new(None);
    status_label.set_xalign(0.0);
    status_label.add_css_class("editor-status");
    status_label.set_hexpand(true);
    chrome.content.append(&status_label);

    let dims_label = gtk::Label::new(None);
    dims_label.set_xalign(0.0);
    dims_label.add_css_class("editor-status");
    chrome.content.append(&dims_label);

    let state = Rc::new(RefCell::new(ImageViewerState {
        paths,
        index,
        load_gen: 0,
        chrome,
        stack,
        picture,
        status_label,
        dims_label,
    }));

    let key_controller = gtk::EventControllerKey::new();
    {
        let state = Rc::clone(&state);
        key_controller.connect_key_pressed(move |_, key, _, _| match key {
            gdk::Key::Escape => {
                state.borrow().chrome.window.close();
                glib::Propagation::Stop
            }
            gdk::Key::Left | gdk::Key::Up | gdk::Key::Page_Up | gdk::Key::BackSpace => {
                navigate(&state, -1);
                glib::Propagation::Stop
            }
            gdk::Key::Right | gdk::Key::Down | gdk::Key::Page_Down | gdk::Key::space => {
                navigate(&state, 1);
                glib::Propagation::Stop
            }
            gdk::Key::Home => {
                jump_to(&state, 0);
                glib::Propagation::Stop
            }
            gdk::Key::End => {
                let last = state.borrow().paths.len().saturating_sub(1);
                jump_to(&state, last);
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        });
    }
    state.borrow().chrome.window.add_controller(key_controller);

    let window = state.borrow().chrome.window.clone();
    let focus = state.borrow().stack.clone();
    present_with_focus(window, focus);

    let initial_index = state.borrow().index;
    jump_to(&state, initial_index);
    Ok(())
}

fn navigate(state: &Rc<RefCell<ImageViewerState>>, delta: isize) {
    let len = state.borrow().paths.len();
    if len <= 1 {
        return;
    }
    let current = state.borrow().index as isize;
    let next = (current + delta).rem_euclid(len as isize) as usize;
    jump_to(state, next);
}

fn jump_to(state: &Rc<RefCell<ImageViewerState>>, index: usize) {
    let (path, gen) = {
        let mut borrowed = state.borrow_mut();
        if index >= borrowed.paths.len() {
            return;
        }
        // Allow reloading the same index only on first open (load_gen == 0).
        if borrowed.load_gen > 0 && index == borrowed.index {
            return;
        }
        borrowed.index = index;
        borrowed.load_gen = borrowed.load_gen.saturating_add(1);
        let gen = borrowed.load_gen;
        let path = borrowed.paths[index].clone();
        set_viewer_path(&borrowed.chrome, &path);
        borrowed.stack.set_visible_child_name("loading");
        borrowed.status_label.set_label(&t!(
            "viewer.status_image_loading",
            mode = t!("viewer.mode_image").into_owned(),
            index = index + 1,
            total = borrowed.paths.len()
        ));
        borrowed
            .dims_label
            .set_label(&t!("viewer.image_loading").into_owned());
        (path, gen)
    };

    let state = Rc::clone(state);
    let (sender, receiver) = std::sync::mpsc::channel::<(u64, Result<DecodedPixels, String>)>();
    thread::spawn(move || {
        let result = load_image_pixels(&path).map_err(|error| error.to_string());
        let _ = sender.send((gen, result));
    });

    glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
        match receiver.try_recv() {
            Ok((result_gen, result)) => {
                match result {
                    Ok(pixels) => match texture_from_pixels(pixels) {
                        Ok(loaded) => apply_loaded(&state, result_gen, loaded),
                        Err(error) => show_load_error(&state, result_gen, &error.to_string()),
                    },
                    Err(error) => show_load_error(&state, result_gen, &error),
                }
                glib::ControlFlow::Break
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
        }
    });
}

fn apply_loaded(state: &Rc<RefCell<ImageViewerState>>, gen: u64, loaded: LoadedImage) {
    let borrowed = state.borrow_mut();
    if gen != borrowed.load_gen {
        return;
    }
    borrowed
        .picture
        .set_paintable(Some(&loaded.texture.clone().upcast::<gdk::Paintable>()));
    borrowed.stack.set_visible_child_name("image");
    refresh_labels(&borrowed, &loaded);
}

fn show_load_error(state: &Rc<RefCell<ImageViewerState>>, gen: u64, error: &str) {
    let borrowed = state.borrow();
    if gen != borrowed.load_gen {
        return;
    }
    borrowed.stack.set_visible_child_name("image");
    borrowed.status_label.set_label(&t!(
        "viewer.error_with_detail",
        error = error
    ));
    borrowed.dims_label.set_label("");
}

fn refresh_labels(state: &ImageViewerState, loaded: &LoadedImage) {
    let path = &state.paths[state.index];
    state.status_label.set_label(&t!(
        "viewer.status_image",
        mode = t!("viewer.mode_image").into_owned(),
        index = state.index + 1,
        total = state.paths.len(),
        size = format_bytes(file_size(path))
    ));
    state.dims_label.set_label(&t!(
        "viewer.image_dimensions",
        width = loaded.width,
        height = loaded.height
    ));
}

fn load_image_pixels(path: &Path) -> Result<DecodedPixels> {
    if is_svg_path(path) {
        return load_svg_pixels(path);
    }

    let decoded = image::open(path)
        .with_context(|| format!("Could not decode image {}", path.display()))?;
    let rgba = decoded.to_rgba8();
    let (width, height) = decoded.dimensions();
    Ok(DecodedPixels {
        rgba: rgba.into_raw(),
        width,
        height,
    })
}

fn load_svg_pixels(path: &Path) -> Result<DecodedPixels> {
    let svg_data = std::fs::read(path)
        .with_context(|| format!("Could not read SVG {}", path.display()))?;

    // Fast path: SVG that is only a wrapper around an embedded PNG/JPEG.
    if let Some(pixels) = try_decode_embedded_raster(&svg_data) {
        return Ok(pixels);
    }

    render_svg_with_resvg(path, &svg_data)
}

fn try_decode_embedded_raster(svg_data: &[u8]) -> Option<DecodedPixels> {
    let text = std::str::from_utf8(svg_data).ok()?;
    // Skip SVGs that clearly contain vector drawing — those need resvg.
    let path_count = text.matches("<path").count() + text.matches("<PATH").count();
    let image_count = text.matches("<image").count() + text.matches("<IMAGE").count();
    if image_count == 0 || path_count > 0 {
        return None;
    }

    let marker = "data:image/";
    let start = text.find(marker)?;
    let after = &text[start..];
    let b64_marker = "base64,";
    let b64_start = after.find(b64_marker)? + b64_marker.len();
    let rest = &after[b64_start..];
    let b64_end = rest
        .find('"')
        .or_else(|| rest.find('\''))
        .or_else(|| rest.find('>'))?;
    let b64: String = rest[..b64_end]
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect();
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64).ok()?;
    let decoded = image::load_from_memory(&bytes).ok()?;
    let rgba = decoded.to_rgba8();
    let (width, height) = decoded.dimensions();
    if width == 0 || height == 0 {
        return None;
    }
    Some(DecodedPixels {
        rgba: rgba.into_raw(),
        width,
        height,
    })
}

fn shared_fontdb() -> std::sync::Arc<resvg::usvg::fontdb::Database> {
    static FONTDB: OnceLock<std::sync::Arc<resvg::usvg::fontdb::Database>> = OnceLock::new();
    FONTDB
        .get_or_init(|| {
            let mut db = resvg::usvg::fontdb::Database::new();
            db.load_system_fonts();
            std::sync::Arc::new(db)
        })
        .clone()
}

fn render_svg_with_resvg(path: &Path, svg_data: &[u8]) -> Result<DecodedPixels> {
    // Small Inkscape icons (e.g. 24×24) look muddy if rendered 1:1 and then
    // upscaled by GTK — rasterize to a sharp viewing size instead.
    const TARGET_EDGE: f32 = 1024.0;
    const MAX_EDGE: f32 = 4096.0;

    let options = resvg::usvg::Options {
        resources_dir: std::fs::canonicalize(path)
            .ok()
            .and_then(|absolute| absolute.parent().map(|parent| parent.to_path_buf())),
        fontdb: shared_fontdb(),
        ..resvg::usvg::Options::default()
    };

    let tree = resvg::usvg::Tree::from_data(svg_data, &options)
        .with_context(|| format!("Could not parse SVG {}", path.display()))?;

    let size = tree.size();
    if size.width() <= 0.0 || size.height() <= 0.0 {
        bail!("{}", t!("viewer.image_decode_error"));
    }

    let longest = size.width().max(size.height());
    let scale = if longest < TARGET_EDGE {
        TARGET_EDGE / longest
    } else if longest > MAX_EDGE {
        MAX_EDGE / longest
    } else {
        1.0
    };
    let width = (size.width() * scale).round().max(1.0) as u32;
    let height = (size.height() * scale).round().max(1.0) as u32;

    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| anyhow::anyhow!("{}", t!("viewer.image_decode_error")))?;
    let transform = resvg::tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    Ok(DecodedPixels {
        rgba: pixmap.take(),
        width,
        height,
    })
}

fn texture_from_pixels(pixels: DecodedPixels) -> Result<LoadedImage> {
    if pixels.width == 0 || pixels.height == 0 {
        bail!("{}", t!("viewer.image_decode_error"));
    }

    let bytes = Bytes::from_owned(pixels.rgba);
    let texture = MemoryTexture::new(
        pixels.width as i32,
        pixels.height as i32,
        MemoryFormat::R8g8b8a8,
        &bytes,
        (pixels.width * 4) as usize,
    );
    Ok(LoadedImage {
        texture,
        width: pixels.width,
        height: pixels.height,
    })
}
