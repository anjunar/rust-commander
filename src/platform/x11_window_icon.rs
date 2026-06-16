use anyhow::{Context, Result};
use gtk::prelude::*;
use image::imageops::FilterType;
use x11rb::{
    connection::Connection,
    properties::WmHints,
    protocol::xproto::{AtomEnum, ConnectionExt as XprotoConnectionExt, PropMode},
    wrapper::ConnectionExt as WrapperConnectionExt,
};

use crate::platform::assets::asset_path;

pub fn apply_window_icon(window: &gtk::ApplicationWindow) -> Result<()> {
    let Some(surface) = window.surface() else {
        return Ok(());
    };
    let Ok(x11_surface) = surface.downcast::<gdk_x11::X11Surface>() else {
        return Ok(());
    };

    let icon_path = asset_path("assets/icons/rust-commander.png");
    let image = image::open(&icon_path)
        .with_context(|| format!("Could not load icon {}", icon_path.display()))?
        .resize(128, 128, FilterType::Lanczos3)
        .into_rgba8();

    let width = image.width();
    let height = image.height();
    let mut icon_data = Vec::with_capacity((width * height + 2) as usize);
    icon_data.push(width);
    icon_data.push(height);
    for pixel in image.pixels() {
        let [r, g, b, a] = pixel.0;
        icon_data
            .push((u32::from(a) << 24) | (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b));
    }

    let (conn, _) = x11rb::connect(None).context("Could not connect to the X11 server")?;
    let net_wm_icon = conn
        .intern_atom(false, b"_NET_WM_ICON")
        .context("Could not request the _NET_WM_ICON atom")?
        .reply()
        .context("Could not resolve the _NET_WM_ICON atom")?
        .atom;

    set_icon_for_window(&conn, x11_surface.xid() as u32, net_wm_icon, &icon_data)?;

    if let Some(group_leader) = WmHints::get(&conn, x11_surface.xid() as u32)
        .context("Could not request WM_HINTS from the X11 window")?
        .reply()
        .context("Could not read WM_HINTS from the X11 window")?
        .and_then(|hints| hints.window_group)
    {
        set_icon_for_window(&conn, group_leader, net_wm_icon, &icon_data)?;
    }

    if let Some(group_surface) = x11_surface.group() {
        if let Ok(group_surface) = group_surface.downcast::<gdk_x11::X11Surface>() {
            set_icon_for_window(&conn, group_surface.xid() as u32, net_wm_icon, &icon_data)?;
        }
    }

    conn.flush()
        .context("Could not flush X11 icon properties")?;
    Ok(())
}

fn set_icon_for_window<C: Connection>(
    conn: &C,
    window: u32,
    net_wm_icon: u32,
    icon_data: &[u32],
) -> Result<()> {
    conn.change_property32(
        PropMode::REPLACE,
        window,
        net_wm_icon,
        AtomEnum::CARDINAL,
        icon_data,
    )
    .with_context(|| format!("Could not set _NET_WM_ICON for X11 window 0x{window:x}"))?;
    Ok(())
}
