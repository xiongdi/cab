#!/usr/bin/env bash
# E2E: claude-code, codex, opencode, grok-build × auto + manual via dev gateway (3125).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DB="${HOME}/.cab/cab.db"
KEY="$(sqlite3 "${DB}" "SELECT json_extract(data, '$.gateway_key') FROM settings WHERE id = 1")"
BASE="http://127.0.0.1:3125"
PROMPT="Reply with exactly: CAB ok"
TIMEOUT="${CAB_E2E_TIMEOUT:-120}"
MODELS_JSON="$(curl -sf -H "Authorization: Bearer ${KEY}" "${BASE}/api/models")"
# Prefer a routable enabled model (auto often picks tencent/hy3 via opencode-go binding).
MANUAL_MODEL="$(echo "${MODELS_JSON}" | python3 -c "
import sys,json
models=json.load(sys.stdin)
for preferred in ('tencent/hy3',):
    for m in models:
        if m.get('name')==preferred and m.get('enabled'):
            print(m['name']); raise SystemExit
for m in models:
    if m.get('enabled') and m.get('provider_id')=='opencode-go':
        print(m['name']); break
else:
    for m in models:
        if m.get('enabled'):
            print(m['name']); break
")"

MANUAL_SUFFIX="${MANUAL_MODEL#*/}"
GROK_MANUAL="cab-$(echo "${MANUAL_MODEL}" | sed 's/[^a-zA-Z0-9._-]/-/g')"

export ANTHROPIC_BASE_URL="${BASE}"
export ANTHROPIC_AUTH_TOKEN="${KEY}"
export CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY=1
export OPENAI_API_KEY="${KEY}"

put_agent() {
  local id="$1" mode="$2" model_id="${3:-}"
  local body
  if [[ "${mode}" == "auto" ]]; then
    body=$(printf '{"mode":"auto","model_id":"%s"}' "${model_id:-auto}")
  else
    body='{"mode":"manual","model_id":null}'
  fi
  curl -sf -X PUT -H "Authorization: Bearer ${KEY}" -H "Content-Type: application/json" \
    -d "${body}" "${BASE}/api/agents/${id}" >/dev/null
  sleep 1
}

latest_log() {
  curl -sf -H "Authorization: Bearer ${KEY}" "${BASE}/api/logs?per_page=1" \
    | python3 -c "import sys,json; d=json.load(sys.stdin).get('data') or []; print(json.dumps(d[0] if d else {}))"
}

run_case() {
  local agent="$1" mode="$2"
  local label="${agent}/${mode}"
  local out rc log_agent log_model

  echo ""
  echo "=== ${label} ==="

  if [[ "${mode}" == "auto" ]]; then
    put_agent "${agent}" auto auto
  else
    put_agent "${agent}" manual
  fi

  local before
  before="$(latest_log | python3 -c "import sys,json; print(json.load(sys.stdin).get('id',''))")"

  set +e
  case "${agent}:${mode}" in
    claude-code:auto)
      out="$(timeout "${TIMEOUT}s" claude -p "${PROMPT}" --model "claude/cab/auto" --max-turns 1 2>&1 </dev/null)"
      rc=$?
      ;;
    claude-code:manual)
      out="$(timeout "${TIMEOUT}s" claude -p "${PROMPT}" --model "claude/cab/${MANUAL_MODEL}" --max-turns 1 2>&1 </dev/null)"
      rc=$?
      ;;
    codex:auto)
      out="$(timeout "${TIMEOUT}s" codex exec "${PROMPT}" 2>&1 </dev/null)"
      rc=$?
      ;;
    codex:manual)
      out="$(timeout "${TIMEOUT}s" codex exec -c "model=\"${MANUAL_MODEL}\"" "${PROMPT}" 2>&1 </dev/null)"
      rc=$?
      ;;
    opencode:auto)
      out="$(cd /tmp && timeout "${TIMEOUT}s" opencode run --pure -m cab/auto "${PROMPT}" 2>&1 </dev/null)"
      rc=$?
      ;;
    opencode:manual)
      out="$(cd /tmp && timeout "${TIMEOUT}s" opencode run --pure -m "cab/${MANUAL_MODEL}" "${PROMPT}" 2>&1 </dev/null)"
      rc=$?
      ;;
    grok-build:auto)
      out="$(timeout "${TIMEOUT}s" grok -p "${PROMPT}" -m cab-auto 2>&1 </dev/null)"
      rc=$?
      ;;
    grok-build:manual)
      out="$(timeout "${TIMEOUT}s" grok -p "${PROMPT}" -m "${GROK_MANUAL}" 2>&1 </dev/null)"
      rc=$?
      ;;
    *)
      echo "FAIL unknown ${label}" >&2
      return 2
      ;;
  esac
  set -e

  log_agent="$(latest_log | python3 -c "import sys,json; print(json.load(sys.stdin).get('agent',''))")"
  log_model="$(latest_log | python3 -c "import sys,json; print(json.load(sys.stdin).get('model_id',''))")"
  after="$(latest_log | python3 -c "import sys,json; print(json.load(sys.stdin).get('id',''))")"

  if [[ ${rc} -eq 124 ]]; then
    pkill -f 'claude|codex|opencode|grok' 2>/dev/null || true
    echo "FAIL ${label}: timeout (${TIMEOUT}s)"
    echo "${out}" | tail -5
    return 1
  fi

  if [[ ${rc} -ne 0 ]]; then
    echo "FAIL ${label}: exit ${rc}"
    echo "${out}" | tail -20
    return 1
  fi

  if [[ "${out}" != *"CAB ok"* && "${out}" != *"cab ok"* ]]; then
    echo "WARN ${label}: CLI ok but output missing 'CAB ok'"
    echo "${out}" | tail -10
  fi

  echo "PASS ${label} rc=${rc} log_agent=${log_agent} resolved=${log_model}"
  if [[ -n "${after}" && "${after}" != "${before}" ]]; then
    echo "  log_id=${after}"
  fi
  echo "${out}" | tail -3 | sed 's/^/  > /'
}

echo "CAB four-agent E2E — base=${BASE} manual_model=${MANUAL_MODEL} grok_manual=${GROK_MANUAL}"
ss -tlnp 2>/dev/null | grep 3125 || { echo "3125 not listening"; exit 1; }

curl -sf -H "Authorization: Bearer ${KEY}" "${BASE}/api/providers" \
  | python3 -c "
import sys,json
ogo=[p for p in json.load(sys.stdin) if p.get('id')=='opencode-go'][0]
print('opencode-go enabled=', ogo.get('enabled'), 'has_key=', bool(ogo.get('api_key')))
"

FAIL=0
for agent in claude-code codex opencode grok-build; do
  for mode in auto manual; do
    run_case "${agent}" "${mode}" || FAIL=$((FAIL + 1))
  done
done

echo ""
if [[ ${FAIL} -eq 0 ]]; then
  echo "ALL 8 CASES PASSED"
  exit 0
fi
echo "${FAIL}/8 CASES FAILED"
exit 1
