#!/usr/bin/env bash
# Build cab-server, apply setcap for 127.0.0.1:443 (agy proxy mode), then run.
# Note: `cargo run` rebuilds the binary and clears setcap — always run the binary directly.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$ROOT/target/debug/cab-server"

cd "$ROOT"
cargo build -p cab-server

if ! getcap "$BIN" 2>/dev/null | grep -q cap_net_bind_service; then
  echo "Applying setcap (polkit prompt may appear)..."
  pkexec setcap cap_net_bind_service=+ep "$BIN"
fi

getcap "$BIN"
exec "$BIN"
