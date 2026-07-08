#!/usr/bin/env bash
# Shared helpers for CAB UAT (packaged server lifecycle + settings).
set -euo pipefail

UAT_PID_FILE="${CAB_UAT_PID_FILE:-/tmp/cab-uat-server.pid}"
UAT_LOG_FILE="${CAB_UAT_LOG_FILE:-/tmp/cab-uat-server.log}"
_UAT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

uat_root() {
  printf '%s' "${_UAT_ROOT}"
}

uat_load_settings() {
  local db="${HOME}/.cab/cab.db"
  if [[ ! -f "${db}" ]]; then
    echo "error: ${db} not found — configure CAB first" >&2
    return 1
  fi
  GATEWAY_PORT="$(sqlite3 "${db}" "SELECT data FROM settings WHERE id=1" | python3 -c "import sys,json; print(json.load(sys.stdin)['gateway_port'])")"
  GATEWAY_KEY="$(sqlite3 "${db}" "SELECT data FROM settings WHERE id=1" | python3 -c "import sys,json; print(json.load(sys.stdin)['gateway_key'])")"
  export GATEWAY_PORT GATEWAY_KEY
  export CAB_UAT_BASE_URL="${CAB_UAT_BASE_URL:-http://127.0.0.1:${GATEWAY_PORT}}"
  export CAB_UAT_GATEWAY_KEY="${CAB_UAT_GATEWAY_KEY:-${GATEWAY_KEY}}"
}

uat_wait_ready() {
  local url="${CAB_UAT_BASE_URL}/api/dashboard/stats"
  local models_url="${CAB_UAT_BASE_URL}/api/models"
  local i
  for i in $(seq 1 180); do
    if curl -sf -H "Authorization: Bearer ${CAB_UAT_GATEWAY_KEY}" "${url}" >/dev/null 2>&1; then
      local count
      count="$(curl -sf -H "Authorization: Bearer ${CAB_UAT_GATEWAY_KEY}" "${models_url}" \
        | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d) if isinstance(d,list) else 0)" 2>/dev/null || echo 0)"
      if [[ "${count}" -gt 0 ]]; then
        return 0
      fi
    fi
    sleep 0.5
  done
  echo "error: packaged CAB server not ready (catalog empty) at ${CAB_UAT_BASE_URL}" >&2
  if [[ -f "${UAT_LOG_FILE}" ]]; then
    echo "--- server log (tail) ---" >&2
    tail -n 40 "${UAT_LOG_FILE}" >&2 || true
  fi
  return 1
}

uat_port_busy() {
  python3 - <<PY
import socket, os, sys
port = int(os.environ["GATEWAY_PORT"])
s = socket.socket()
try:
    s.bind(("127.0.0.1", port))
except OSError:
    sys.exit(0)
else:
    sys.exit(1)
finally:
    s.close()
PY
}

uat_build_release() {
  local root
  root="$(uat_root)"
  echo "== UAT: building release cab-srv =="
  (cd "${root}" && cargo build --release -p cab-srv)
  export CAB_UAT_SERVER_BIN="${root}/target/release/cab-srv"
}

uat_stop_managed_server() {
  if [[ ! -f "${UAT_PID_FILE}" ]]; then
    return 0
  fi
  local pid
  pid="$(cat "${UAT_PID_FILE}" 2>/dev/null || true)"
  if [[ -n "${pid}" ]] && kill -0 "${pid}" 2>/dev/null; then
    echo "== UAT: stopping managed server (pid ${pid}) =="
    kill "${pid}" 2>/dev/null || true
    for _ in $(seq 1 40); do
      kill -0 "${pid}" 2>/dev/null || break
      sleep 0.25
    done
    kill -9 "${pid}" 2>/dev/null || true
  fi
  rm -f "${UAT_PID_FILE}"
}

uat_start_packaged_server() {
  uat_load_settings
  uat_build_release

  if [[ "${CAB_UAT_REUSE_RUNNING:-0}" == "1" ]]; then
    echo "== UAT: reusing running server at ${CAB_UAT_BASE_URL} =="
    uat_wait_ready
    return 0
  fi

  uat_stop_managed_server

  if uat_port_busy; then
    echo "error: gateway port ${GATEWAY_PORT} already in use." >&2
    echo "Stop the running CAB instance, or set CAB_UAT_REUSE_RUNNING=1 to reuse it." >&2
    return 1
  fi

  local root
  root="$(uat_root)"
  echo "== UAT: starting packaged server ${CAB_UAT_SERVER_BIN} on port ${GATEWAY_PORT} =="
  : >"${UAT_LOG_FILE}"
  cd "${root}"
  nohup "${CAB_UAT_SERVER_BIN}" >>"${UAT_LOG_FILE}" 2>&1 &
  echo $! >"${UAT_PID_FILE}"
  cd - >/dev/null
  uat_wait_ready
  echo "== UAT: packaged server ready at ${CAB_UAT_BASE_URL} =="
}
