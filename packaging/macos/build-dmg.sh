#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

package_name="rust-commander"
app_name="RCommander"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)"
arch="${MACOS_ARCH:-$(uname -m)}"
package_root="$repo_root/target/packages/${package_name}_${version}_macos-${arch}"
stage_root="$package_root/stage"
app_bundle="$stage_root/${app_name}.app"
dmg_root="$package_root/dmg"
dmg_path="$package_root/${app_name}-${version}-macos-${arch}.dmg"

"$repo_root/packaging/macos/stage-runtime.sh"

rm -rf "$dmg_root"
mkdir -p "$dmg_root"
cp -R "$app_bundle" "$dmg_root/${app_name}.app"
ln -s /Applications "$dmg_root/Applications"
rm -f "$dmg_path"

hdiutil create \
    -volname "$app_name" \
    -srcfolder "$dmg_root" \
    -format UDZO \
    "$dmg_path"

echo
echo "Built distribution:"
echo "  $dmg_path"
