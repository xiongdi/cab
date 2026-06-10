#!/usr/bin/env bash
# Validate the management API OpenAPI baseline used for v0.2.0.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SPEC="$ROOT/spec/src/content/docs/modules/openapi.yaml"

python3 - <<'PY' "$SPEC"
import sys
from pathlib import Path

try:
    import yaml
except ImportError:
    print("PyYAML is required to validate openapi.yaml", file=sys.stderr)
    sys.exit(1)

path = Path(sys.argv[1])
data = yaml.safe_load(path.read_text())
assert data.get("openapi"), "missing openapi version"
assert data.get("paths"), "missing paths"
required = ["/api/routing/explain", "/api/logs", "/api/agents/{id}"]
for route in required:
    if route not in data["paths"]:
        raise SystemExit(f"missing path: {route}")
print(f"OpenAPI baseline OK: {path}")
PY
