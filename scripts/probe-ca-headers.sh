#!/usr/bin/env bash
# Probe locally installed coding agents: capture real HTTP headers on a mock gateway.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CAPTURE_PY="${ROOT}/scripts/capture-agent-headers.py"
OUT_FILE="${ROOT}/.cab-ca-header-probe.jsonl"
REPORT="${ROOT}/.cab-ca-header-probe-report.json"

export CAB_CAPTURE_FILE="$OUT_FILE"
rm -f "$OUT_FILE" "$REPORT"

start_capture() {
  CAPTURE_LOG="$(mktemp)"
  python3 "$CAPTURE_PY" 0 >"$CAPTURE_LOG" 2>&1 &
  CAPTURE_PID=$!
  for _ in $(seq 1 50); do
    if [[ -s "$CAPTURE_LOG" ]]; then
      CAPTURE_PORT="$(python3 -c "import json; print(json.load(open('$CAPTURE_LOG'))['port'])")"
      CAPTURE_BASE="http://127.0.0.1:${CAPTURE_PORT}"
      return 0
    fi
    sleep 0.1
  done
  echo "capture server failed to start" >&2
  kill "$CAPTURE_PID" 2>/dev/null || true
  exit 1
}

stop_capture() {
  kill "$CAPTURE_PID" 2>/dev/null || true
  wait "$CAPTURE_PID" 2>/dev/null || true
}

tag_next_capture() {
  export CAB_PROBE_TAG="$1"
}

run_probe() {
  local id="$1"
  shift
  tag_next_capture "$id"
  printf '[probe] %s\n' "$id"
  if "$@"; then
    printf '%s\n' "$id ok"
  else
    printf '%s\n' "$id fail" >&2
  fi
  sleep 0.3
}

probe_claude() {
  command -v claude >/dev/null || return 1
  ANTHROPIC_BASE_URL="${CAPTURE_BASE}" \
  ANTHROPIC_API_KEY=probe-key \
  ANTHROPIC_AUTH_TOKEN=probe-key \
  timeout 60s claude -p "reply with exactly: ok" --max-turns 1 >/dev/null 2>&1
}

probe_codex() {
  command -v codex >/dev/null || return 1
  OPENAI_API_KEY=probe-key \
  timeout 90s codex exec \
    -c 'model_provider="probe"' \
    -c 'model_providers.probe.name="probe"' \
    -c "model_providers.probe.base_url=\"${CAPTURE_BASE}/v1\"" \
    -c 'model_providers.probe.env_key="OPENAI_API_KEY"' \
    -c 'model_providers.probe.wire_api="chat"' \
    -c 'model="gpt-4o-mini"' \
    "reply with exactly: ok" >/dev/null 2>&1
}

probe_opencode() {
  command -v opencode >/dev/null || return 1
  local cfg
  cfg="$(mktemp --suffix=.json)"
  cat >"$cfg" <<EOF
{
  "\$schema": "https://opencode.ai/config.json",
  "provider": {
    "probe": {
      "name": "probe",
      "npm": "@ai-sdk/openai-compatible",
      "options": {
        "baseURL": "${CAPTURE_BASE}/v1",
        "apiKey": "probe-key"
      },
      "models": {
        "test": { "name": "test" }
      }
    }
  }
}
EOF
  timeout 90s opencode run --pure -c "$cfg" -m probe/test "reply with exactly: ok" >/dev/null 2>&1
  local rc=$?
  rm -f "$cfg"
  return $rc
}

probe_hermes_openai() {
  command -v hermes >/dev/null || return 1
  local cfg
  cfg="$(mktemp --suffix=.yaml)"
  cat >"$cfg" <<EOF
model:
  provider: custom
  default: test
  model: test
  base_url: ${CAPTURE_BASE}/v1
  api_key: probe-key
  api_mode: chat_completions
EOF
  timeout 90s env HERMES_CONFIG_FILE="$cfg" hermes chat -q "reply with exactly: ok" >/dev/null 2>&1
  local rc=$?
  rm -f "$cfg"
  return $rc
}

probe_hermes_anthropic() {
  command -v hermes >/dev/null || return 1
  local cfg
  cfg="$(mktemp --suffix=.yaml)"
  cat >"$cfg" <<EOF
model:
  provider: anthropic
  default: claude-sonnet-4-6
  api_key: probe-key
EOF
  # Point Anthropic adapter at capture server via env override if supported
  timeout 90s env \
    HERMES_CONFIG_FILE="$cfg" \
    ANTHROPIC_BASE_URL="${CAPTURE_BASE}" \
    hermes chat -q "reply with exactly: ok" >/dev/null 2>&1
  local rc=$?
  rm -f "$cfg"
  return $rc
}

main() {
  start_capture
  printf '[probe] capture %s\n' "$CAPTURE_BASE"

  run_probe claude-code probe_claude || true
  run_probe codex probe_codex || true
  run_probe opencode probe_opencode || true
  run_probe hermes-openai probe_hermes_openai || true
  run_probe hermes-anthropic probe_hermes_anthropic || true

  for id in kilocode openclaw pi; do
    if command -v "$id" >/dev/null 2>&1 || command -v "${id%-code}" >/dev/null 2>&1; then
      printf '[probe] %s installed but no automated probe yet\n' "$id"
    else
      printf '[probe] %s not on PATH\n' "$id"
    fi
  done

  sleep 0.5
  stop_capture

  python3 - <<PY >"$REPORT"
import json
from pathlib import Path

out = Path("$OUT_FILE")
rows = []
if out.exists():
    for line in out.read_text(encoding="utf-8").splitlines():
        if line.strip():
            rows.append(json.loads(line))

supported = {
    "claude-code": "claude-code",
    "codex": "codex",
    "opencode": "opencode",
    "hermes": "hermes",
    "kilocode": "kilocode",
    "openclaw": "openclaw",
    "pi": "pi",
}

def interesting(headers):
    keys = ["user-agent", "User-Agent", "originator", "x-title", "anthropic-version"]
    return {k: headers[k] for k in headers if k.lower() in keys or k.lower().startswith("x-")}

report = []
for row in rows:
    h = row.get("headers", {})
    ua = h.get("user-agent") or h.get("User-Agent") or ""
    cab = row.get("cab_extract_agent", "")
    mapped = cab in supported.values()
    report.append({
        "path": row.get("path"),
        "user_agent": ua,
        "originator": h.get("originator") or h.get("Originator"),
        "cab_extract_agent": cab,
        "maps_to_supported_id": mapped,
        "notable_headers": interesting(h),
    })

print(json.dumps({"capture_base": "$CAPTURE_BASE", "requests": report}, indent=2, ensure_ascii=False))
PY

  cat "$REPORT"
}

main "$@"
