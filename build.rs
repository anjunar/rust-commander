fn main() {
    // Only run on Windows
    #[cfg(target_os = "windows")]
    {
        if let Err(e) = try_set_win_icon() {
            println!("cargo:warning=Failed to set Windows icon: {}", e);
        }
    }
}

#[cfg(target_os = "windows")]
fn try_set_win_icon() -> Result<(), Box<dyn std::error::Error>> {
    let icon_path = "assets/icons/app_icon.ico";
    if std::path::Path::new(icon_path).exists() {
        let mut res = winres::WindowsResource::new();
        res.set_icon(icon_path);
        res.compile()?;
    } else {
        println!(
            "cargo:warning=Icon file {} not found, skipping resource embed",
            icon_path
        );
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn _foo() {}
