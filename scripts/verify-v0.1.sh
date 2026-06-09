#!/usr/bin/env bash
# Back-compat entry: delegates to honest layered test runner.
set -euo pipefail
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/run-tests.sh"
