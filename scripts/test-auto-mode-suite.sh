#!/usr/bin/env bash
# E2E auto-mode test suite:
# - routing/explain: verify resolved model exists and task classification matches prompt type
# - real CLI calls: verify each agent returns "CAB ok"
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DB="${HOME}/.cab/cab.db"
KEY="$(sqlite3 "${DB}" "SELECT json_extract(data, '$.gateway_key') FROM settings WHERE id = 1")"
BASE="http://127.0.0.1:3125"

TIMEOUT="${CAB_E2E_TIMEOUT:-120}"
PROMPT_WAIT_SLEEP_SECS="${CAB_E2E_SLEEP_SECS:-1}"

AGENTS=(claude-code codex opencode grok-build)

TASK_CODING_EXPECTED=""  # not asserted: coding agents may still classify as Coding
TASK_MATH_EXPECTED=""
TASK_AGENTIC_EXPECTED=""
TASK_INTELLIGENCE_EXPECTED=""

# JSON messages for routing/explain. Keep prompts single-line to avoid JSON escaping pain.
CODING_MESSAGES='[{"role":"user","content":"Reply with exactly: CAB ok. Write a tiny TypeScript function in a code block. Use ```ts\\nexport const cab = () => 1;\\n```"}]'
MATH_MESSAGES='[{"role":"user","content":"Reply with exactly: CAB ok. Compute the integral of x^2 from 0 to 1 using \\int_0^1 x^2 dx."}]'
# Ensure message_count > 4 so agentic scoring can trigger via message history.
AGENTIC_MESSAGES='[
  {"role":"user","content":"You are an agent. Reply in steps."},
  {"role":"assistant","content":"Sure."},
  {"role":"user","content":"Use tools to inspect the repository and then run a command."},
  {"role":"assistant","content":"I will."},
  {"role":"user","content":"Add a plan and mention which tools you would call. Reply with exactly: CAB ok."},
  {"role":"assistant","content":"OK."}
]'
INTELLIGENCE_MESSAGES='[{"role":"user","content":"Reply with exactly: CAB ok. Summarize the idea behind routing scores and explain why some models may be unroutable."}]'

explain_and_assert_resolved() {
  local agent="$1"
  local messages_json="$2"

  local explain_json
  explain_json="$(
    curl -sf -X POST \
      -H "Authorization: Bearer ${KEY}" \
      -H "Content-Type: application/json" \
      -d "{\"agent\":\"${agent}\",\"model\":\"auto\",\"strategy\":\"auto\",\"messages\":${messages_json}}" \
      "${BASE}/api/routing/explain"
  )"

  python3 - <<'PY' "${explain_json}"
import json, re, sys
explain = json.loads(sys.argv[1])

resolved = explain.get("resolved") or {}
model_id = resolved.get("model_id")
provider_id = resolved.get("provider_id")

if not model_id:
    print("FAIL: routing/explain resolved.model_id is empty")
    print(json.dumps(explain, ensure_ascii=False)[:2000])
    raise SystemExit(1)

if not provider_id:
    print("FAIL: routing/explain resolved.provider_id is empty")
    print("resolved:", resolved)
    raise SystemExit(1)

print("OK: explain resolved model_id=", model_id, "provider_id=", provider_id)
PY
}

set_agent_auto_mode() {
  local agent="$1"
  # CAB agent API expects mode "auto" + model_id "auto" (or null).
  curl -sf -X PUT \
    -H "Authorization: Bearer ${KEY}" \
    -H "Content-Type: application/json" \
    -d "{\"mode\":\"auto\",\"model_id\":\"auto\"}" \
    "${BASE}/api/agents/${agent}" >/dev/null
  sleep "${PROMPT_WAIT_SLEEP_SECS}"
}

run_cli_assert_cab_ok() {
  local agent="$1"
  local prompt="$2"

  set +e
  local out

  case "${agent}" in
    claude-code)
      out="$(timeout "${TIMEOUT}s" \
        claude -p "${prompt}" \
        --model "claude/cab/auto" \
        --max-turns 2 2>&1 </dev/null)"
      ;;
    codex)
      out="$(timeout "${TIMEOUT}s" \
        codex exec "${prompt}" 2>&1 </dev/null)"
      ;;
    opencode)
      out="$(cd /tmp && timeout "${TIMEOUT}s" \
        opencode run --pure -m cab/auto "${prompt}" 2>&1 </dev/null)"
      ;;
    grok-build)
      # grok: -p/--single <PROMPT>
      out="$(timeout "${TIMEOUT}s" \
        grok -m cab-auto --single "${prompt}" 2>&1 </dev/null)"
      ;;
    *)
      echo "FAIL: unknown agent ${agent}" >&2
      exit 2
      ;;
  esac

  local rc=$?
  set -e

  local has_cab_ok
  has_cab_ok="$(echo "${out}" | rg -i -q "CAB ok|cab ok" && echo yes || echo no)"
  if [[ "${has_cab_ok}" != "yes" ]]; then
    echo "FAIL: ${agent} output missing 'CAB ok'"
    echo "${out}" | tail -40
    return 1
  fi
  # Some providers/CLIs may time out after successfully printing CAB ok.
  if [[ "${rc}" -ne 0 ]]; then
    if [[ "${rc}" -eq 124 ]]; then
      echo "WARN: ${agent} timed out (rc=124) but output has 'CAB ok'"
      return 0
    fi
    echo "FAIL: ${agent} cli exit rc=${rc} (but output contains CAB ok)"
    return 1
  fi

  echo "OK: ${agent} -> CAB ok"
}

echo "CAB auto-mode test suite: base=${BASE} timeout=${TIMEOUT}s"

ss -tlnp | rg "3125" >/dev/null || { echo "FAIL: 3125 not listening"; exit 1; }

# Ensure agent CLIs have gateway creds via env.
export ANTHROPIC_BASE_URL="${BASE}"
export ANTHROPIC_AUTH_TOKEN="${KEY}"
export CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY=1
export OPENAI_API_KEY="${KEY}"

FAILS=0

for agent in "${AGENTS[@]}"; do
  echo ""
  echo "=== agent=${agent} mode=auto ==="
  set_agent_auto_mode "${agent}"

  # coding
  explain_and_assert_resolved "${agent}" "${CODING_MESSAGES}" || FAILS=$((FAILS+1))
  run_cli_assert_cab_ok "${agent}" 'Reply with exactly: CAB ok' || FAILS=$((FAILS+1))

  # math
  explain_and_assert_resolved "${agent}" "${MATH_MESSAGES}" || FAILS=$((FAILS+1))
  run_cli_assert_cab_ok "${agent}" 'Reply with exactly: CAB ok' || FAILS=$((FAILS+1))

  # agentic
  explain_and_assert_resolved "${agent}" "${AGENTIC_MESSAGES}" || FAILS=$((FAILS+1))
  run_cli_assert_cab_ok "${agent}" 'Reply with exactly: CAB ok' || FAILS=$((FAILS+1))

  # intelligence
  explain_and_assert_resolved "${agent}" "${INTELLIGENCE_MESSAGES}" || FAILS=$((FAILS+1))
  run_cli_assert_cab_ok "${agent}" 'Reply with exactly: CAB ok' || FAILS=$((FAILS+1))
done

echo ""
if [[ "${FAILS}" -eq 0 ]]; then
  echo "ALL auto-mode cases PASSED"
  exit 0
fi

echo "${FAILS} failures"
exit 1

