#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "Building cab and cabd in release mode..."
cargo build --release -p cab -p cab-server

mkdir -p ~/.local/bin

echo "Installing cab and cabd to ~/.local/bin/..."
cp "$ROOT/target/release/cab" ~/.local/bin/
cp "$ROOT/target/release/cabd" ~/.local/bin/

echo "Installing cabd systemd user service..."
~/.local/bin/cab service install

echo "Successfully installed cab and cabd!"
