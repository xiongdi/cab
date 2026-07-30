#!/usr/bin/env bash
# Package cab-cli + cab-srv + UI into a release archive for the curl installer.
#
# Usage:
#   ./scripts/package-cli.sh <os> <arch> [outdir]
#
# Examples:
#   ./scripts/package-cli.sh linux x64
#   ./scripts/package-cli.sh darwin arm64 dist/cli
#
# Expects:
#   resources/bin/cab-cli[.exe]
#   resources/bin/cab-srv[.exe]
#   resources/ui/   (or build/ as fallback)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OS="${1:?usage: package-cli.sh <os> <arch> [outdir]}"
ARCH="${2:?usage: package-cli.sh <os> <arch> [outdir]}"
OUTDIR="${3:-${ROOT}/dist/cli}"

case "$OS" in
  linux|darwin|windows) ;;
  *) echo "os must be linux|darwin|windows" >&2; exit 1 ;;
esac
case "$ARCH" in
  x64|arm64) ;;
  *) echo "arch must be x64|arm64" >&2; exit 1 ;;
esac

mkdir -p "$OUTDIR"
# CI may pass a relative outdir; canonicalize before we `cd` into the stage tree.
OUTDIR="$(cd "$OUTDIR" && pwd)"

ext=""
archive_ext="tar.gz"
if [[ "$OS" == "windows" ]]; then
  ext=".exe"
  archive_ext="zip"
fi

bin_dir="${ROOT}/resources/bin"
ui_src="${ROOT}/resources/ui"
[[ -d "$ui_src" ]] || ui_src="${ROOT}/build"

cli="${bin_dir}/cab-cli${ext}"
srv="${bin_dir}/cab-srv${ext}"

[[ -f "$cli" ]] || { echo "missing $cli — run npm run tauri:pre-build first" >&2; exit 1; }
[[ -f "$srv" ]] || { echo "missing $srv — run npm run tauri:pre-build first" >&2; exit 1; }
[[ -d "$ui_src" ]] || { echo "missing UI at $ui_src — run npm run build first" >&2; exit 1; }

stage="${OUTDIR}/stage-cab-${OS}-${ARCH}"
rm -rf "$stage"
mkdir -p "$stage/ui"
cp "$cli" "$stage/"
cp "$srv" "$stage/"
# copy UI contents into stage/ui
cp -R "${ui_src}/." "$stage/ui/"

asset="cab-${OS}-${ARCH}.${archive_ext}"
out_path="${OUTDIR}/${asset}"
rm -f "$out_path"

(
  cd "$stage"
  # Write to parent of stage (== OUTDIR) via a relative path so PowerShell on
  # Windows Git Bash does not see an MSYS path like /d/a/... (invalid for Win32).
  if [[ "$archive_ext" == "tar.gz" ]]; then
    tar -czf "../${asset}" "cab-cli${ext}" "cab-srv${ext}" ui
  elif command -v zip >/dev/null 2>&1; then
    zip -qr "../${asset}" "cab-cli${ext}" "cab-srv${ext}" ui
  else
    powershell.exe -NoProfile -NonInteractive -Command \
      "Compress-Archive -Path 'cab-cli${ext}','cab-srv${ext}','ui' -DestinationPath '..\\${asset}' -Force"
  fi
)

rm -rf "$stage"
echo "Wrote $out_path"
ls -lh "$out_path"
