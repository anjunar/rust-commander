use gtk::{gdk, prelude::*};
use image::imageops::FilterType;

use crate::platform::assets::asset_path;

const SPLASH_TITLE: &str = "RCommander";
const SPLASH_IMAGE_WIDTH: u32 = 520;
const SPLASH_IMAGE_HEIGHT: u32 = 320;

pub struct StartupSplash {
    window: gtk::ApplicationWindow,
}

impl StartupSplash {
    pub fn new(app: &gtk::Application) -> Self {
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title(SPLASH_TITLE)
            .build();
        window.set_decorated(false);
        window.set_resizable(false);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.set_hexpand(true);
        root.set_vexpand(true);
        root.set_halign(gtk::Align::Fill);
        root.set_valign(gtk::Align::Fill);
        root.add_css_class("splash-overlay");

        let content = gtk::Box::new(gtk::Orientation::Vertical, 16);
        content.set_margin_top(24);
        content.set_margin_bottom(24);
        content.set_margin_start(24);
        content.set_margin_end(24);
        content.set_valign(gtk::Align::Center);
        content.set_halign(gtk::Align::Center);
        content.add_css_class("splash-card");

        let image_path = asset_path("assets/splash.png");
        if image_path.exists() {
            if let Some(texture) = load_scaled_texture(&image_path) {
                let picture = gtk::Picture::for_paintable(&texture);
                picture.set_halign(gtk::Align::Center);
                picture.set_valign(gtk::Align::Center);
                picture.set_can_shrink(false);
                content.append(&picture);
            }
        }

        let title = gtk::Label::new(Some(SPLASH_TITLE));
        title.add_css_class("app-title");
        content.append(&title);

        let spinner = gtk::Spinner::new();
        spinner.set_spinning(true);
        spinner.set_visible(true);
        content.append(&spinner);

        let subtitle = gtk::Label::new(Some("Loading panels..."));
        subtitle.add_css_class("status-line");
        content.append(&subtitle);

        root.append(&content);
        window.set_child(Some(&root));

        Self { window }
    }

    pub fn present(&self) {
        self.window.present();
        self.window.fullscreen();
    }

    pub fn close(&self) {
        self.window.close();
    }
}

fn load_scaled_texture(path: &std::path::Path) -> Option<gdk::Texture> {
    let image = image::open(path).ok()?;
    let resized = image.resize(
        SPLASH_IMAGE_WIDTH,
        SPLASH_IMAGE_HEIGHT,
        FilterType::Lanczos3,
    );
    let rgba = resized.to_rgba8();
    let width = rgba.width() as i32;
    let height = rgba.height() as i32;
    let stride = (rgba.width() * 4) as usize;
    let bytes = gtk::glib::Bytes::from_owned(rgba.into_raw());
    let texture = gdk::MemoryTexture::new(
        width,
        height,
        gdk::MemoryFormat::R8g8b8a8,
        &bytes,
        stride,
    );
    Some(texture.upcast())
}
