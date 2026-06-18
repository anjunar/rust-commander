#!/usr/bin/env bash

set -euo pipefail
shopt -s nullglob

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

package_name="rust-commander"
app_name="RCommander"
bundle_id="dev.rcommander.Gtk"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)"
arch="${MACOS_ARCH:-$(uname -m)}"
macports_prefix="${MACPORTS_PREFIX:-}"
deployment_target="${MACOSX_DEPLOYMENT_TARGET:-12.0}"

if [[ -z "$macports_prefix" ]]; then
    if [[ -d /opt/local ]]; then
        macports_prefix="/opt/local"
    else
        echo "Could not determine MacPorts prefix. Set MACPORTS_PREFIX." >&2
        exit 1
    fi
fi

package_root="$repo_root/target/packages/${package_name}_${version}_macos-${arch}"
stage_root="$package_root/stage"
app_bundle="$stage_root/${app_name}.app"
contents_dir="$app_bundle/Contents"
macos_dir="$contents_dir/MacOS"
frameworks_dir="$contents_dir/Frameworks"
resources_dir="$contents_dir/Resources"
bundle_bin="$macos_dir/$app_name"
manifest_path="$stage_root/stage-manifest.json"
iconset_dir="$package_root/iconset"
icnsset_dir="$package_root/${app_name}.iconset"
source_png="$repo_root/assets/icons/dev.rcommander.Gtk.png"
app_icon="$resources_dir/${app_name}.icns"
release_binary="$repo_root/target/release/rust-commander"
resource_bin_dir="$resources_dir/bin"

declare -a helper_bins=(
    "gdk-pixbuf-query-loaders"
    "gtk4-query-immodules-4.0"
)

copy_tree() {
    local source_path="$1"
    local destination_path="$2"

    if [[ ! -e "$source_path" ]]; then
        echo "Required path not found: $source_path" >&2
        exit 1
    fi

    mkdir -p "$(dirname "$destination_path")"
    rm -rf "$destination_path"
    cp -R "$source_path" "$destination_path"
}

copy_tree_if_present() {
    local source_path="$1"
    local destination_path="$2"

    if [[ -e "$source_path" ]]; then
        copy_tree "$source_path" "$destination_path"
    fi
}

resolve_dependency_path() {
    local source_file="$1"
    local dependency="$2"
    local source_dir
    local dep_name

    source_dir="$(cd "$(dirname "$source_file")" && pwd)"

    if [[ "$dependency" == @loader_path/* ]]; then
        local relative_path="${dependency#@loader_path/}"
        local resolved="$source_dir/$relative_path"
        [[ -e "$resolved" ]] && printf '%s\n' "$resolved" && return 0
    fi

    if [[ "$dependency" == @executable_path/* ]]; then
        local relative_path="${dependency#@executable_path/}"
        local resolved="$macos_dir/$relative_path"
        [[ -e "$resolved" ]] && printf '%s\n' "$resolved" && return 0
    fi

    if [[ "$dependency" = /* && -e "$dependency" ]]; then
        printf '%s\n' "$dependency"
        return 0
    fi

    dep_name="$(basename "$dependency")"
    for candidate in \
        "$macports_prefix/lib/$dep_name" \
        "$macports_prefix/libexec/"*/lib/"$dep_name"
    do
        if [[ -e "$candidate" ]]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done

    return 1
}

is_system_dependency() {
    local dependency="$1"
    [[ "$dependency" == /System/Library/* ]] \
        || [[ "$dependency" == /usr/lib/* ]] \
        || [[ "$dependency" == /Library/Apple/System/* ]]
}

rewrite_dependencies() {
    local target_file="$1"
    chmod u+w "$target_file" 2>/dev/null || true

    if [[ "$target_file" == *.dylib ]]; then
        install_name_tool -id "@executable_path/../Frameworks/$(basename "$target_file")" "$target_file"
    fi
}

bundle_file_closure() {
    local queue=("$@")
    local -A seen=()

    while [[ "${#queue[@]}" -gt 0 ]]; do
        local current="${queue[0]}"
        queue=("${queue[@]:1}")

        local current_real
        current_real="$(python3 - <<'PY' "$current"
import os, sys
print(os.path.realpath(sys.argv[1]))
PY
)"

        if [[ -n "${seen[$current_real]:-}" ]]; then
            continue
        fi
        seen["$current_real"]=1

        rewrite_dependencies "$current"

        while IFS= read -r dep; do
            [[ -z "$dep" ]] && continue
            if is_system_dependency "$dep"; then
                continue
            fi
            if [[ "$dep" == @executable_path/../Frameworks/* ]]; then
                continue
            fi

            local resolved
            if ! resolved="$(resolve_dependency_path "$current" "$dep" 2>/dev/null)"; then
                continue
            fi

            local dep_name
            dep_name="$(basename "$resolved")"
            local dep_real
            dep_real="$(python3 - <<'PY' "$resolved"
import os, sys
print(os.path.realpath(sys.argv[1]))
PY
)"

            local destination="$frameworks_dir/$dep_name"
            if [[ ! -e "$destination" ]]; then
                cp "$resolved" "$destination"
                chmod u+w "$destination" 2>/dev/null || true
                queue+=("$destination")
            fi

            install_name_tool -change "$dep" "@executable_path/../Frameworks/$dep_name" "$current"
        done < <(otool -L "$current" | tail -n +2 | awk '{print $1}')
    done
}

create_info_plist() {
    cat > "$contents_dir/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleDisplayName</key>
    <string>${app_name}</string>
    <key>CFBundleExecutable</key>
    <string>${app_name}</string>
    <key>CFBundleIconFile</key>
    <string>${app_name}</string>
    <key>CFBundleIdentifier</key>
    <string>${bundle_id}</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>${app_name}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>${version}</string>
    <key>CFBundleVersion</key>
    <string>${version}</string>
    <key>LSMinimumSystemVersion</key>
    <string>${deployment_target}</string>
    <key>NSDesktopFolderUsageDescription</key>
    <string>RCommander needs access to your Desktop folder to browse and open files.</string>
    <key>NSDocumentsFolderUsageDescription</key>
    <string>RCommander needs access to your Documents folder to browse and open files.</string>
    <key>NSDownloadsFolderUsageDescription</key>
    <string>RCommander needs access to your Downloads folder to browse and open files.</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
EOF
}

create_icns() {
    rm -rf "$icnsset_dir"
    mkdir -p "$icnsset_dir"

    for size in 16 32 64 128 256 512; do
        sips -z "$size" "$size" "$source_png" --out "$icnsset_dir/icon_${size}x${size}.png" >/dev/null
        local retina_size=$((size * 2))
        sips -z "$retina_size" "$retina_size" "$source_png" --out "$icnsset_dir/icon_${size}x${size}@2x.png" >/dev/null
    done

    iconutil -c icns "$icnsset_dir" -o "$app_icon"
}

compile_bundle_metadata() {
    if [[ -d "$resources_dir/share/glib-2.0/schemas" ]] && command -v glib-compile-schemas >/dev/null 2>&1; then
        glib-compile-schemas "$resources_dir/share/glib-2.0/schemas"
    fi

    if command -v gdk-pixbuf-query-loaders >/dev/null 2>&1; then
        local pixbuf_modules_dir
        pixbuf_modules_dir="$(find "$resources_dir/lib/gdk-pixbuf-2.0" -type d -path '*/loaders' -print -quit 2>/dev/null || true)"
        if [[ -n "$pixbuf_modules_dir" ]]; then
            local pixbuf_modules=("$pixbuf_modules_dir"/*.so "$pixbuf_modules_dir"/*.dylib)
            if [[ "${#pixbuf_modules[@]}" -gt 0 ]]; then
                PATH="$resource_bin_dir:$PATH" \
                DYLD_FALLBACK_LIBRARY_PATH="$frameworks_dir${DYLD_FALLBACK_LIBRARY_PATH:+:$DYLD_FALLBACK_LIBRARY_PATH}" \
                gdk-pixbuf-query-loaders "${pixbuf_modules[@]}" > "$resources_dir/lib/gdk-pixbuf-2.0/2.10.0/loaders.cache"
            fi
        fi
    fi

    if command -v gtk4-query-immodules-4.0 >/dev/null 2>&1; then
        local immodules_dir
        immodules_dir="$(find "$resources_dir/lib/gtk-4.0" -type d -path '*/immodules' -print -quit 2>/dev/null || true)"
        if [[ -n "$immodules_dir" ]]; then
            local immodules=("$immodules_dir"/*.so "$immodules_dir"/*.dylib)
            if [[ "${#immodules[@]}" -gt 0 ]]; then
                PATH="$resource_bin_dir:$PATH" \
                DYLD_FALLBACK_LIBRARY_PATH="$frameworks_dir${DYLD_FALLBACK_LIBRARY_PATH:+:$DYLD_FALLBACK_LIBRARY_PATH}" \
                gtk4-query-immodules-4.0 "${immodules[@]}" > "$resources_dir/lib/gtk-4.0/gtk.immodules"
            fi
        fi
    fi
}

macos_rustflags="${RUSTFLAGS:-}"
if [[ -n "$macos_rustflags" ]]; then
    macos_rustflags+=" "
fi
macos_rustflags+="-C link-arg=-Wl,-headerpad_max_install_names"

MACOSX_DEPLOYMENT_TARGET="$deployment_target" \
RUSTFLAGS="$macos_rustflags" \
cargo build --release --bin rust-commander

rm -rf "$package_root"
mkdir -p \
    "$macos_dir" \
    "$frameworks_dir" \
    "$resources_dir" \
    "$resource_bin_dir"

cp "$release_binary" "$bundle_bin"
chmod 0755 "$bundle_bin"
create_info_plist
create_icns

copy_tree "$repo_root/assets" "$resources_dir/assets"
copy_tree "$macports_prefix/share/glib-2.0" "$resources_dir/share/glib-2.0"
copy_tree "$macports_prefix/share/gtk-4.0" "$resources_dir/share/gtk-4.0"
copy_tree "$macports_prefix/share/gtksourceview-5" "$resources_dir/share/gtksourceview-5"
copy_tree "$macports_prefix/share/icons/Adwaita" "$resources_dir/share/icons/Adwaita"
copy_tree "$macports_prefix/share/icons/hicolor" "$resources_dir/share/icons/hicolor"
copy_tree_if_present "$macports_prefix/share/themes/Default" "$resources_dir/share/themes/Default"
copy_tree "$macports_prefix/lib/gdk-pixbuf-2.0" "$resources_dir/lib/gdk-pixbuf-2.0"
copy_tree_if_present "$macports_prefix/lib/gio/modules" "$resources_dir/lib/gio/modules"
copy_tree_if_present "$macports_prefix/lib/gtk-4.0" "$resources_dir/lib/gtk-4.0"

for helper in "${helper_bins[@]}"; do
    helper_path="$macports_prefix/bin/$helper"
    if [[ -x "$helper_path" ]]; then
        cp "$helper_path" "$resource_bin_dir/$helper"
        chmod 0755 "$resource_bin_dir/$helper"
    fi
done

bundle_targets=("$bundle_bin")
while IFS= read -r file; do
    bundle_targets+=("$file")
done < <(find "$resources_dir/lib" "$resource_bin_dir" -type f \( -name '*.dylib' -o -name '*.so' -o -perm -111 \) 2>/dev/null | sort)

bundle_file_closure "${bundle_targets[@]}"
compile_bundle_metadata

cat > "$manifest_path" <<EOF
{
  "productName": "${app_name}",
  "version": "${version}",
  "stageRoot": "$(cd "$stage_root" && pwd)",
  "appBundle": "${app_name}.app",
  "macPortsPrefix": "${macports_prefix}",
  "deploymentTarget": "${deployment_target}",
  "arch": "${arch}"
}
EOF

echo
echo "Staged macOS runtime:"
echo "  $app_bundle"
