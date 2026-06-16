#!/usr/bin/env bash

set -euo pipefail

if ! command -v apt-get >/dev/null 2>&1; then
    echo "This setup script currently supports Ubuntu/Debian systems with apt-get." >&2
    exit 1
fi

packages=(
    build-essential
    pkg-config
    libgtk-4-dev
    libgraphene-1.0-dev
    libgtksourceview-5-dev
    libvte-2.91-gtk4-dev
    libunrar-dev
)

echo "Installing Ubuntu build dependencies for rust-commander..."
sudo apt-get update
sudo apt-get install -y "${packages[@]}"

echo
echo "System dependencies installed."
echo "Next steps:"
echo "  cargo check"
echo "  cargo run --bin rust-commander"
