#!/usr/bin/env bash

set -euo pipefail

app_id="dev.rcommander.Gtk"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
desktop_src="$repo_root/packaging/linux/${app_id}.desktop"

desktop_dir="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
icon_root="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor"
pixmaps_dir="${XDG_DATA_HOME:-$HOME/.local/share}/pixmaps"
bin_dir="${HOME}/.local/bin"
icon_stage="$repo_root/target/iconset/local"

mkdir -p "$desktop_dir" "$icon_root" "$pixmaps_dir" "$bin_dir"

launcher_path="$bin_dir/rcommander-dev"
binary_path="$repo_root/target/debug/rcommander"
if [ ! -x "$binary_path" ]; then
    cargo build --bin rcommander
fi
cargo run --quiet --bin generate_icon -- --output-dir "$icon_stage"
cat > "$launcher_path" <<EOF
#!/usr/bin/env bash
set -euo pipefail
cd "$repo_root"
exec "$binary_path"
EOF
chmod 0755 "$launcher_path"

escaped_exec="$(printf '%s' "$launcher_path" | sed 's/[&|]/\\&/g')"
sed "s|@EXEC@|$escaped_exec|g" "$desktop_src" > "$desktop_dir/${app_id}.desktop"
chmod 0644 "$desktop_dir/${app_id}.desktop"

cp -a "$icon_stage/hicolor/." "$icon_root/"
cp -a "$icon_stage/pixmaps/." "$pixmaps_dir/"
rm -f "$desktop_dir/rcommander.desktop"

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$desktop_dir" || true
fi

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "$icon_root" || true
fi

echo "Installed:"
echo "  $desktop_dir/${app_id}.desktop"
echo "  $icon_root"
echo "  $pixmaps_dir"
echo "  $launcher_path"
echo
echo "Start once via:"
echo "  gtk-launch ${app_id}"
