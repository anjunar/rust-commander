#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

package_name="rust-commander"
app_id="dev.rcommander.Gtk"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)"
arch="$(dpkg --print-architecture)"
depends="${DEB_DEPENDS:-libgtk-4-1, libgtksourceview-5-0, libvte-2.91-gtk4-0, libunrar5t64}"

cargo build --release --bin rust-commander

stage_root="$repo_root/target/packages/${package_name}_${version}_${arch}"
deb_root="$stage_root/pkg"
icon_stage="$stage_root/iconset"
rm -rf "$stage_root"

mkdir -p \
    "$deb_root/DEBIAN" \
    "$deb_root/usr/bin" \
    "$deb_root/usr/share/applications" \
    "$deb_root/usr/share/icons/hicolor" \
    "$deb_root/usr/share/pixmaps"

cargo run --quiet --bin generate_icon -- --output-dir "$icon_stage"

install -m 0755 "$repo_root/target/release/rust-commander" "$deb_root/usr/bin/rust-commander"
cp -a "$icon_stage/hicolor/." "$deb_root/usr/share/icons/hicolor/"
cp -a "$icon_stage/pixmaps/." "$deb_root/usr/share/pixmaps/"

sed 's|@EXEC@|rust-commander|g' \
    "$repo_root/packaging/linux/${app_id}.desktop" \
    > "$deb_root/usr/share/applications/${app_id}.desktop"
chmod 0644 "$deb_root/usr/share/applications/${app_id}.desktop"

installed_size="$(du -sk "$deb_root" | cut -f1)"
sed \
    -e "s|@VERSION@|$version|g" \
    -e "s|@ARCH@|$arch|g" \
    -e "s|@DEPENDS@|$depends|g" \
    -e "s|@INSTALLED_SIZE@|$installed_size|g" \
    "$repo_root/packaging/linux/debian-control" \
    > "$deb_root/DEBIAN/control"

install -m 0755 "$repo_root/packaging/linux/debian-postinst" "$deb_root/DEBIAN/postinst"
install -m 0755 "$repo_root/packaging/linux/debian-postrm" "$deb_root/DEBIAN/postrm"

desktop-file-validate "$deb_root/usr/share/applications/${app_id}.desktop"
dpkg-deb --build --root-owner-group "$deb_root" "$stage_root/${package_name}_${version}_${arch}.deb"

echo
echo "Built package:"
echo "  $stage_root/${package_name}_${version}_${arch}.deb"
