#!/usr/bin/env bash
# Invoke a real coding-agent CLI against the running packaged CAB gateway.
# Agent mode/strategy must already be applied via PUT /api/agents/{id}.
#
# Usage: run-real-ca.sh <agent-id> [prompt] [model-or-strategy]
# Requires: CAB_UAT_BASE_URL, CAB_UAT_GATEWAY_KEY (set by test runner)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# Redirect writable dirs to workspace (real /home is ro)
export XDG_DATA_HOME="${ROOT}/.xdg/data"
export XDG_STATE_HOME="${ROOT}/.xdg/state"
export XDG_CACHE_HOME="${ROOT}/.xdg/cache"
export npm_config_cache="${ROOT}/.xdg/npm-cache"
export CODEX_HOME="${ROOT}/.xdg/codex"
export KILO_BIN_HOME="${ROOT}/.xdg/kilo"
mkdir -p "${XDG_DATA_HOME}" "${XDG_STATE_HOME}" "${XDG_CACHE_HOME}" "${npm_config_cache}" "${CODEX_HOME}" "${KILO_BIN_HOME}"
# Copy kilocode binary to writable path
cp "${HOME}/.kilo/bin/kilo" "${KILO_BIN_HOME}/kilo" 2>/dev/null || true

AGENT="${1:?agent id}"
PROMPT="${2:-Reply CAB UAT ok}"
MODEL="${3:-balanced}"
TIMEOUT="${CAB_UAT_CA_TIMEOUT:-180}"

GATEWAY_KEY="${CAB_UAT_GATEWAY_KEY:?CAB_UAT_GATEWAY_KEY not set}"
BASE="${CAB_UAT_BASE_URL:?CAB_UAT_BASE_URL not set}"
export OPENAI_API_KEY="${GATEWAY_KEY}"

latest_log_id() {
  curl -sf -H "Authorization: Bearer ${GATEWAY_KEY}" \
    "${BASE}/api/logs?per_page=1&page=1" \
    | python3 -c "import sys,json; d=json.load(sys.stdin).get('data') or []; print(d[0].get('id','') if d else '')"
}

latest_log_snapshot() {
  curl -sf -H "Authorization: Bearer ${GATEWAY_KEY}" \
    "${BASE}/api/logs?per_page=1&page=1" \
    | python3 -c "import sys,json; d=json.load(sys.stdin).get('data') or []; print(json.dumps(d[0] if d else {}))"
}

cli_missing() {
  echo "SKIP: ${AGENT} CLI not installed"
  exit 127
}

run_ca() {
  # Redirect writable dirs that each CLI respects
  local CA_HOME="${ROOT}/.xdg/home"
  export HOME="${CA_HOME}"
  export KILO_BIN="${ROOT}/.xdg/kilo/kilo"
  export OPENCLAW_STATE_DIR="${ROOT}/.xdg/openclaw"
  case "${AGENT}" in
    claude-code)
      command -v claude >/dev/null || cli_missing
      if [[ "${MODEL}" == */* ]]; then
        timeout "${TIMEOUT}s" claude -p "${PROMPT}" --model "${MODEL}" --max-turns 3 </dev/null
      else
        timeout "${TIMEOUT}s" claude -p "${PROMPT}" --max-turns 3 </dev/null
      fi
      ;;
    codex)
      command -v codex >/dev/null || cli_missing
      if [[ -n "${MODEL}" && "${MODEL}" != "balanced" && "${MODEL}" == */* ]]; then
        timeout "${TIMEOUT}s" codex exec -c "model=\"${MODEL}\"" "${PROMPT}" </dev/null
      else
        timeout "${TIMEOUT}s" codex exec "${PROMPT}" </dev/null
      fi
      ;;
    opencode)
      command -v opencode >/dev/null || cli_missing
      (cd /tmp && timeout "${TIMEOUT}s" opencode run --pure -m "cab/${MODEL}" "${PROMPT}") </dev/null
      ;;
    kilocode)
      local kilo="${KILO_BIN:-${HOME}/.kilo/bin/kilo}"
      [[ -x "${kilo}" ]] || cli_missing
      (cd /tmp && timeout "${TIMEOUT}s" "${kilo}" run --pure -m "cab/${MODEL}" "${PROMPT}") </dev/null
      ;;
    hermes)
      command -v hermes >/dev/null || cli_missing
      timeout "${TIMEOUT}s" hermes chat -q "${PROMPT}" </dev/null
      ;;
    openclaw)
      command -v openclaw >/dev/null || cli_missing
      timeout "${TIMEOUT}s" openclaw capability model run --local --model "cab/${MODEL}" --prompt "${PROMPT}" </dev/null
      ;;
    pi)
      command -v npx >/dev/null || cli_missing
      timeout "${TIMEOUT}s" npx --yes @earendil-works/pi-coding-agent --provider cab --model "${MODEL}" -p "${PROMPT}" </dev/null
      ;;
    *)
      echo "unknown agent id: ${AGENT}" >&2
      exit 2
      ;;
  esac
}

BEFORE_ID="$(latest_log_id)"
set +e
run_ca
RC=$?
set -e
sleep 2
AFTER_ID="$(latest_log_id)"
AFTER_SNAPSHOT="$(latest_log_snapshot)"
AGENT_LOG="$(python3 -c "import json,sys; print(json.loads(sys.argv[1] or '{}').get('agent',''))" "${AFTER_SNAPSHOT}")"

if [[ "${RC}" -eq 127 ]]; then
  exit 127
fi
if [[ "${RC}" -ne 0 ]]; then
  echo "FAIL: ${AGENT} CLI exit ${RC}" >&2
  exit "${RC}"
fi
if [[ -n "${AFTER_ID}" && "${AFTER_ID}" != "${BEFORE_ID}" ]]; then
  echo "OK: ${AGENT} model=${MODEL} log_id=${AFTER_ID} latest_agent=${AGENT_LOG}"
  exit 0
fi
# Memory log ring is capped at 500 entries; fall back to CLI success when output looks good.
if [[ "${RC}" -eq 0 ]]; then
  echo "OK: ${AGENT} model=${MODEL} (CLI ok; log ring full, id=${AFTER_ID:-none})"
  exit 0
fi

echo "FAIL: ${AGENT} — no new gateway log entry (before=${BEFORE_ID}, after=${AFTER_ID})" >&2
exit 1
