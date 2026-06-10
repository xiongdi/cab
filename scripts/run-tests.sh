#!/usr/bin/env bash
# CI gate: backend UT / IT / ST (no UAT — use ./scripts/run-uat.sh locally with real keys).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "== UT: Backend unit tests (Rust lib) =="
cargo test -p cab-db -p cab-api -p cab-core -p cab-gateway --lib --quiet

echo ""
echo "== IT: Integration tests (management API, in-process) =="
cargo test -p cab-api --test agents_it --test api_it --quiet

echo ""
echo "== ST: System tests (combined router + TCP) =="
echo "  ST-a: in-process wiring"
cargo test -p cab-server --test system_v01 --quiet
echo "  ST-b: real HTTP on ephemeral port"
cargo test -p cab-server --test system_tcp --quiet

echo ""
echo "== Regression: full Rust workspace (UAT tests ignored, not run here) =="
cargo test --workspace --quiet

echo ""
echo "== OpenAPI baseline validation =="
bash "$ROOT/scripts/generate-openapi.sh"

echo ""
echo "Gate passed: UT → IT → ST."
echo "UAT is local-only: ./scripts/run-uat.sh (requires ~/.cab/settings.json + real API keys)"
