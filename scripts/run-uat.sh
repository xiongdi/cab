#!/usr/bin/env bash
# Local UAT only — NOT part of CI gate. Uses real API keys from SQLite (~/.cab/cab.db).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [[ ! -f "${HOME}/.cab/cab.db" ]]; then
  echo "error: ~/.cab/cab.db not found — configure providers in CAB first" >&2
  exit 1
fi

REPORT_DIR="${ROOT}/reports/uat"
mkdir -p "${REPORT_DIR}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
export CAB_UAT_REPORT="${REPORT_DIR}/uat-${STAMP}.md"
export CAB_VERSION="$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')"

# shellcheck source=scripts/uat/lib.sh
source "${ROOT}/scripts/uat/lib.sh"

cleanup() {
  uat_stop_managed_server
}
trap cleanup EXIT

uat_start_packaged_server
export CAB_RUN_UAT=1
export CAB_UAT_CA_TIMEOUT="${CAB_UAT_CA_TIMEOUT:-300}"

echo "== UAT: packaged cab-server at ${CAB_UAT_BASE_URL} =="
echo "== UAT: real CA CLIs (claude, codex, opencode, …) =="
echo "== Report: ${CAB_UAT_REPORT} =="

set +e
cargo test -p cab-server --test uat_scenarios -- --test-threads=1 --ignored --nocapture
TEST_EXIT=$?
set -e

if [[ -f "${REPORT_DIR}/latest.md" ]]; then
  echo ""
  echo "========== UAT Report Summary =========="
  awk '/^## Summary$/,0' "${REPORT_DIR}/latest.md" || true
  echo "========================================"
  echo "Full report: ${REPORT_DIR}/latest.md"
  echo "This run:    ${CAB_UAT_REPORT}"
fi

if [[ ${TEST_EXIT} -ne 0 ]]; then
  echo "UAT failed (exit ${TEST_EXIT})" >&2
  exit "${TEST_EXIT}"
fi

echo ""
echo "UAT passed (UAT-07 desktop shell remains manual)."
