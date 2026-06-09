#!/usr/bin/env bash
# Usage: probe-one-ca.sh <agent-id>
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ID="${1:?agent id}"
OUT="/tmp/cab-probe-${ID}.jsonl"
META="/tmp/cab-probe-${ID}-meta.json"
rm -f "$OUT"
CAB_CAPTURE_FILE="$OUT" python3 "${ROOT}/scripts/capture-agent-headers.py" 0 >"$META" &
PID=$!
sleep 0.8
PORT="$(python3 -c "import json; print(json.load(open('$META'))['port'])")"
CAPTURE_BASE="http://127.0.0.1:${PORT}"
printf '[probe:%s] %s\n' "$ID" "$CAPTURE_BASE"

run_probe() {
  case "$ID" in
    claude-code)
      ANTHROPIC_BASE_URL="$CAPTURE_BASE" \
      ANTHROPIC_API_KEY="sk-ant-api03-probe-probe-probe-probe-probe-probe-probe-probe-probe-probe-probe" \
      ANTHROPIC_AUTH_TOKEN="sk-ant-api03-probe-probe-probe-probe-probe-probe-probe-probe-probe-probe-probe" \
      timeout 45s claude -p "reply exactly: ok" </dev/null
      ;;
    codex)
      OPENAI_API_KEY=probe-key timeout 60s codex exec \
        -c 'model_provider="probe"' \
        -c 'model_providers.probe.name="probe"' \
        -c "model_providers.probe.base_url=\"${CAPTURE_BASE}/v1\"" \
        -c 'model_providers.probe.env_key="OPENAI_API_KEY"' \
        -c 'model_providers.probe.wire_api="responses"' \
        -c 'model="gpt-4o-mini"' \
        "reply exactly: ok" </dev/null
      ;;
    opencode)
      cfg="${HOME}/.config/opencode/opencode.json"
      bak="${cfg}.cab-probe-bak"
      cp "$cfg" "$bak"
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
      timeout 60s opencode run --pure -m probe/test "reply exactly: ok" </dev/null || true
      mv "$bak" "$cfg"
      ;;
    hermes-openai)
      cfg="${HOME}/.hermes/config.yaml"
      bak="${cfg}.cab-probe-bak"
      cp "$cfg" "$bak"
      cat >"$cfg" <<EOF
model:
  provider: custom
  default: test
  model: test
  base_url: ${CAPTURE_BASE}/v1
  api_key: probe-key
  api_mode: chat_completions
EOF
      timeout 60s hermes chat -q "reply exactly: ok" </dev/null || true
      mv "$bak" "$cfg"
      ;;
    hermes-anthropic)
      cfg="${HOME}/.hermes/config.yaml"
      bak="${cfg}.cab-probe-bak"
      cp "$cfg" "$bak"
      cat >"$cfg" <<EOF
model:
  provider: anthropic
  default: claude-sonnet-4-6
  api_key: sk-ant-api03-probe-probe-probe-probe-probe-probe-probe-probe-probe-probe-probe
EOF
      ANTHROPIC_BASE_URL="$CAPTURE_BASE" \
      ANTHROPIC_API_KEY="sk-ant-api03-probe-probe-probe-probe-probe-probe-probe-probe-probe-probe-probe" \
      timeout 60s hermes chat -q "reply exactly: ok" </dev/null || true
      mv "$bak" "$cfg"
      ;;
    pi)
      models="${HOME}/.pi/agent/models.json"
      bak="${models}.cab-probe-bak"
      mkdir -p "$(dirname "$models")"
      [[ -f "$models" ]] && cp "$models" "$bak"
      cat >"$models" <<EOF
{
  "providers": {
    "probe": {
      "baseUrl": "${CAPTURE_BASE}/v1",
      "api": "openai-completions",
      "apiKey": "probe-key",
      "models": [{ "id": "test", "name": "test" }]
    }
  }
}
EOF
      timeout 60s npx --yes @earendil-works/pi-coding-agent --provider probe --model test -p "reply exactly: ok" </dev/null || true
      if [[ -f "$bak" ]]; then mv "$bak" "$models"; else rm -f "$models"; fi
      ;;
    kilocode)
      cfg="${HOME}/.config/kilo/opencode.json"
      bak="${cfg}.cab-probe-bak"
      mkdir -p "$(dirname "$cfg")"
      [[ -f "$cfg" ]] && cp "$cfg" "$bak"
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
      KILO_BIN="${KILO_BIN:-${HOME}/.kilo/bin/kilo}"
      timeout 90s "$KILO_BIN" run --pure -m probe/test "reply exactly: ok" </dev/null || true
      if [[ -f "$bak" ]]; then mv "$bak" "$cfg"; else rm -f "$cfg"; fi
      ;;
    kilocode-headers)
      cfg="${HOME}/.config/kilo/opencode.json"
      bak="${cfg}.cab-probe-bak"
      mkdir -p "$(dirname "$cfg")"
      [[ -f "$cfg" ]] && cp "$cfg" "$bak"
      cat >"$cfg" <<EOF
{
  "\$schema": "https://opencode.ai/config.json",
  "provider": {
    "probe": {
      "name": "probe",
      "npm": "@ai-sdk/openai-compatible",
      "options": {
        "baseURL": "${CAPTURE_BASE}/v1",
        "apiKey": "probe-key",
        "headers": {
          "User-Agent": "KiloCode/7.3.40 (cab-probe)",
          "X-CAB-Agent": "kilocode"
        }
      },
      "models": {
        "test": { "name": "test" }
      }
    }
  }
}
EOF
      KILO_BIN="${KILO_BIN:-${HOME}/.kilo/bin/kilo}"
      timeout 90s "$KILO_BIN" run --pure -m probe/test "reply exactly: ok" </dev/null || true
      if [[ -f "$bak" ]]; then mv "$bak" "$cfg"; else rm -f "$cfg"; fi
      ;;
    openclaw)
      openclaw config unset models.providers.probe >/dev/null 2>&1 || true
      openclaw config set models.providers.probe "$(cat <<EOF
{"baseUrl":"${CAPTURE_BASE}/v1","apiKey":"probe-key","api":"openai-completions","models":[{"id":"test","name":"test"}]}
EOF
)" --strict-json --merge
      timeout 90s openclaw capability model run --local --model probe/test --prompt "reply exactly: ok" </dev/null || true
      openclaw config unset models.providers.probe >/dev/null 2>&1 || true
      ;;
    openclaw-headers)
      openclaw config unset models.providers.probe >/dev/null 2>&1 || true
      openclaw config set models.providers.probe "$(cat <<EOF
{"baseUrl":"${CAPTURE_BASE}/v1","apiKey":"probe-key","api":"openai-completions","headers":{"User-Agent":"OpenClaw/2026.6.1 (cab-probe)","X-CAB-Agent":"openclaw"},"models":[{"id":"test","name":"test"}]}
EOF
)" --strict-json --merge
      timeout 90s openclaw capability model run --local --model probe/test --prompt "reply exactly: ok" </dev/null || true
      openclaw config unset models.providers.probe >/dev/null 2>&1 || true
      ;;
    *)
      echo "unknown agent id: $ID" >&2
      exit 2
      ;;
  esac
}

set +e
run_probe
RC=$?
set -e
sleep 0.5
kill "$PID" 2>/dev/null || true
wait "$PID" 2>/dev/null || true

python3 - <<PY
import json
from pathlib import Path
p = Path("$OUT")
rows = [json.loads(l) for l in p.read_text(encoding="utf-8").splitlines() if l.strip()] if p.exists() else []
api = [r for r in rows if "/v1/" in r.get("path", "")]
pick = api[0] if api else (rows[0] if rows else None)
if not pick:
    print(json.dumps({"id": "$ID", "exit_code": $RC, "requests": 0}, indent=2))
else:
    h = pick["headers"]
    ua = h.get("user-agent") or h.get("User-Agent") or ""
    print(json.dumps({
        "id": "$ID",
        "exit_code": $RC,
        "requests": len(rows),
        "api_requests": len(api),
        "path": pick.get("path"),
        "user_agent": ua,
        "originator": h.get("originator") or h.get("Originator"),
        "cab_extract_agent": pick.get("cab_extract_agent"),
        "notable_headers": {k:v for k,v in h.items() if k.lower() in {"user-agent","originator","anthropic-version"} or k.lower().startswith("x-")},
    }, indent=2, ensure_ascii=False))
PY
