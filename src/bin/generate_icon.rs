use std::{fs::File, path::Path};
use anyhow::Result;
use image::{io::Reader as ImageReader, imageops::FilterType};
use ico::{IconDir, IconImage, IconDirEntry};

fn main() -> Result<()> {
    let src = Path::new("assets/icons/0acc2ce6-257e-4031-9332-b5c73960f871.png");
    let out = Path::new("assets/icons/app_icon.ico");

    if !src.exists() {
        anyhow::bail!("Source PNG not found: {}", src.display());
    }

    let img = ImageReader::open(src)?.decode()?.into_rgba8();

    let sizes = [256u32, 128, 64, 48, 32, 16];
    let mut dir = IconDir::new(ico::ResourceType::Icon);

    for &size in &sizes {
        let resized = image::imageops::resize(&img, size, size, FilterType::Lanczos3);
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
