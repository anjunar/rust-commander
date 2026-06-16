#!/usr/bin/env bash

set -euo pipefail

app_id="dev.rcommander.Gtk"
desktop_id="rust-commander"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
desktop_src="$repo_root/packaging/linux/${app_id}.desktop"
icon_src="$repo_root/assets/icons/${desktop_id}.png"

desktop_dir="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
icon_dir="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/1024x1024/apps"
bin_dir="${HOME}/.local/bin"

mkdir -p "$desktop_dir" "$icon_dir" "$bin_dir"

launcher_path="$bin_dir/${desktop_id}-dev"
binary_path="$repo_root/target/debug/rust-commander"
if [ ! -x "$binary_path" ]; then
    cargo build --bin rust-commander
fi
cat > "$launcher_path" <<EOF
#!/usr/bin/env bash
set -euo pipefail
cd "$repo_root"
exec "$binary_path"
EOF
chmod 0755 "$launcher_path"

escaped_exec="$(printf '%s' "$launcher_path" | sed 's/[&|]/\\&/g')"
sed "s|@EXEC@|$escaped_exec|g" "$desktop_src" > "$desktop_dir/${desktop_id}.desktop"
chmod 0644 "$desktop_dir/${desktop_id}.desktop"

install -m 0644 "$icon_src" "$icon_dir/${desktop_id}.png"

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$desktop_dir" || true
fi

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "$(dirname "$(dirname "$icon_dir")")" || true
fi

echo "Installed:"
echo "  $desktop_dir/${desktop_id}.desktop"
echo "  $icon_dir/${desktop_id}.png"
echo "  $launcher_path"
echo
echo "Start once via:"
echo "  gtk-launch ${desktop_id}"
