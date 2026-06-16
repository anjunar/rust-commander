use anyhow::Result;
use ico::{IconDir, IconDirEntry, IconImage};
use image::{imageops::FilterType, io::Reader as ImageReader};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};

const SOURCE_PNG: &str = "assets/icons/dev.rcommander.Gtk.png";
const WINDOWS_ICO: &str = "assets/icons/app_icon.ico";
const LINUX_ICON_NAMES: &[&str] = &["dev.rcommander.Gtk", "rust-commander"];
const LINUX_ICON_SIZES: &[u32] = &[16, 22, 24, 32, 48, 64, 128, 256, 512];

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let output_dir = match (args.next().as_deref(), args.next()) {
        (Some("--output-dir"), Some(path)) => Some(PathBuf::from(path)),
        (None, None) => None,
        _ => anyhow::bail!("Usage: cargo run --bin generate_icon -- [--output-dir <dir>]"),
    };
    let src = Path::new(SOURCE_PNG);

    if !src.exists() {
        anyhow::bail!("Source PNG not found: {}", src.display());
    }

    let img = ImageReader::open(src)?.decode()?.into_rgba8();
    write_windows_ico(&img)?;

    if let Some(output_dir) = output_dir {
        write_linux_icons(&img, &output_dir)?;
    }

    Ok(())
}

fn write_windows_ico(img: &image::RgbaImage) -> Result<()> {
    let out = Path::new(WINDOWS_ICO);
    let sizes = [256u32, 128, 64, 48, 32, 16];
    let mut dir = IconDir::new(ico::ResourceType::Icon);

    for &size in &sizes {
        let resized = image::imageops::resize(img, size, size, FilterType::Lanczos3);
        let (w, h) = (resized.width() as u32, resized.height() as u32);
        let rgba = resized.into_raw();
        let entry = IconImage::from_rgba_data(w, h, rgba);
        let encoded = IconDirEntry::encode(&entry)?;
        dir.add_entry(encoded);
    }

    let mut file = File::create(out)?;
    dir.write(&mut file)?;

    println!("Wrote {}", out.display());
    Ok(())
}

fn write_linux_icons(img: &image::RgbaImage, output_dir: &Path) -> Result<()> {
    for &size in LINUX_ICON_SIZES {
        let resized = image::imageops::resize(img, size, size, FilterType::Lanczos3);
        let icon_dir = output_dir.join(format!("hicolor/{size}x{size}/apps"));
        fs::create_dir_all(&icon_dir)?;

        for &icon_name in LINUX_ICON_NAMES {
            let icon_path = icon_dir.join(format!("{icon_name}.png"));
            resized.save(&icon_path)?;
            println!("Wrote {}", icon_path.display());
        }
    }

    let pixmaps_dir = output_dir.join("pixmaps");
    fs::create_dir_all(&pixmaps_dir)?;
    for &icon_name in LINUX_ICON_NAMES {
        let pixmap_path = pixmaps_dir.join(format!("{icon_name}.png"));
        img.save(&pixmap_path)?;
        println!("Wrote {}", pixmap_path.display());
    }

    Ok(())
}
