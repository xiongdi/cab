#!/usr/bin/env bash
# UAT smoke: v0.1.0 scope — 7 agents, no proxy mode, HTTP gateway only.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "== UAT-0: Workspace compile =="
cargo build --workspace --quiet

echo "== UAT-1: Unit tests (UT) =="
cargo test -p cab-db -p cab-api --lib --quiet
cargo test -p cab-core -p cab-gateway --lib --quiet

echo "== UAT-2: Integration tests (IT) =="
cargo test -p cab-api --test agents_it --quiet

echo "== UAT-3: System tests (ST) =="
cargo test -p cab-server --test system_v01 --quiet

echo "== UAT-4: Full workspace regression =="
cargo test --workspace --quiet

echo "== UAT-5: Frontend typecheck =="
if command -v npm >/dev/null 2>&1 && [[ -f package.json && -x node_modules/.bin/svelte-check ]]; then
  npx svelte-kit sync >/dev/null 2>&1 || true
  npm run check --silent
else
  echo "skip: frontend dependencies not installed"
fi

echo ""
echo "UAT passed: 7-agent catalog, proxy endpoints removed, gateway HTTP routes OK."
