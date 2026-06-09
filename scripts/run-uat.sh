#!/usr/bin/env bash
# Local UAT only — NOT part of CI gate. Uses real API keys from ~/.cab/settings.json.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [[ ! -f "${HOME}/.cab/settings.json" ]]; then
  echo "error: ~/.cab/settings.json not found — configure providers in CAB first" >&2
  exit 1
fi

export CAB_RUN_UAT=1
echo "== UAT: real keys from ~/.cab/settings.json, ephemeral local port =="
cargo test -p cab-server --test uat_scenarios -- --test-threads=1 --ignored --nocapture
echo ""
echo "UAT passed (UAT-07 desktop shell remains manual)."
