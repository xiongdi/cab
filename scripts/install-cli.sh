#!/usr/bin/env bash
# Build release cab and install to ~/.local/bin
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "Building cab in release mode..."
cargo build --release -p cab

mkdir -p ~/.local/bin
echo "Installing cab to ~/.local/bin/..."
cp "$ROOT/target/release/cab" ~/.local/bin/
chmod 755 ~/.local/bin/cab

echo "Installing cab systemd user service..."
~/.local/bin/cab service install
~/.local/bin/cab start || true

echo "Successfully installed cab!"
echo "Open dashboard: cab gui"
