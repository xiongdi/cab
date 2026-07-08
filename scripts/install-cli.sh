#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "Building cab-cli and cab-srv in release mode..."
cargo build --release -p cab -p cab-srv

mkdir -p ~/.local/bin

echo "Installing cab-cli and cab-srv to ~/.local/bin/..."
cp "$ROOT/target/release/cab-cli" ~/.local/bin/
cp "$ROOT/target/release/cab-srv" ~/.local/bin/

echo "Installing cab-srv systemd user service..."
~/.local/bin/cab-cli service install

echo "Successfully installed cab-cli and cab-srv!"
