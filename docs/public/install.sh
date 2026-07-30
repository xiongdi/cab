#!/usr/bin/env bash
# CAB one-line installer (cab + dashboard UI).
#
#   curl -fsSL https://raw.githubusercontent.com/xiongdi/cab/main/scripts/install.sh | bash
#   curl -fsSL https://xiongdi.github.io/cab/install.sh | bash
#
# Options (pass after -- when piping):
#   --version <ver>     Install a specific version (e.g. 0.9.0 or v0.9.0)
#   --dir <path>        Install root (default: ~/.cab)
#   --no-modify-path    Do not edit shell rc files
#   --no-service        Skip `cab service install`
#   --help
set -euo pipefail

REPO="${CAB_REPO:-xiongdi/cab}"
INSTALL_ROOT="${CAB_INSTALL_ROOT:-${HOME}/.cab}"
REQUESTED_VERSION="${CAB_VERSION:-}"
NO_MODIFY_PATH=false
NO_SERVICE=false

MUTED=$'\033[0;2m'
RED=$'\033[0;31m'
GREEN=$'\033[0;32m'
NC=$'\033[0m'

usage() {
  cat <<EOF
Install CAB (`cab`) from GitHub Releases.

Usage:
  curl -fsSL https://raw.githubusercontent.com/xiongdi/cab/main/scripts/install.sh | bash
  curl -fsSL …/install.sh | bash -s -- --version 0.9.0

Options:
  -v, --version <ver>   Install this version (with or without leading v)
  -d, --dir <path>      Install root (default: ~/.cab)
      --no-modify-path  Do not modify shell config for PATH
      --no-service      Do not run: cab service install
  -h, --help            Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage; exit 0 ;;
    -v|--version)
      [[ -n "${2:-}" ]] || { echo "${RED}Error: --version needs an argument${NC}" >&2; exit 1; }
      REQUESTED_VERSION="$2"; shift 2 ;;
    -d|--dir)
      [[ -n "${2:-}" ]] || { echo "${RED}Error: --dir needs an argument${NC}" >&2; exit 1; }
      INSTALL_ROOT="$2"; shift 2 ;;
    --no-modify-path) NO_MODIFY_PATH=true; shift ;;
    --no-service) NO_SERVICE=true; shift ;;
    *)
      echo "${MUTED}Warning: unknown option '$1'${NC}" >&2
      shift ;;
  esac
done

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "${RED}Error: required command not found: $1${NC}" >&2
    exit 1
  }
}

need_cmd curl
need_cmd uname

raw_os=$(uname -s)
case "$raw_os" in
  Darwin*) os=darwin ;;
  Linux*) os=linux ;;
  MINGW*|MSYS*|CYGWIN*) os=windows ;;
  *)
    echo "${RED}Unsupported OS: $raw_os${NC}" >&2
    exit 1 ;;
esac

arch=$(uname -m)
case "$arch" in
  x86_64|amd64) arch=x64 ;;
  aarch64|arm64) arch=arm64 ;;
  *)
    echo "${RED}Unsupported arch: $arch${NC}" >&2
    exit 1 ;;
esac

# Apple Silicon under Rosetta reports x86_64 — prefer native arm64.
if [[ "$os" == "darwin" && "$arch" == "x64" ]]; then
  if [[ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" == "1" ]]; then
    arch=arm64
  fi
fi

if [[ "$os" == "windows" ]]; then
  archive_ext=zip
  need_cmd unzip
else
  archive_ext=tar.gz
  need_cmd tar
fi

asset="cab-${os}-${arch}.${archive_ext}"

api="https://api.github.com/repos/${REPO}/releases"
if [[ -z "$REQUESTED_VERSION" ]]; then
  echo "${MUTED}Fetching latest release…${NC}"
  release_json=$(curl -fsSL "${api}/latest")
  tag=$(printf '%s' "$release_json" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1)
  [[ -n "$tag" ]] || { echo "${RED}Failed to resolve latest release tag${NC}" >&2; exit 1; }
else
  tag="${REQUESTED_VERSION#v}"
  tag="v${tag}"
  echo "${MUTED}Fetching release ${tag}…${NC}"
  release_json=$(curl -fsSL "${api}/tags/${tag}") || {
    echo "${RED}Release ${tag} not found${NC}" >&2
    exit 1
  }
fi

version="${tag#v}"
url="https://github.com/${REPO}/releases/download/${tag}/${asset}"

echo "${MUTED}Installing CAB ${NC}${version}${MUTED} (${os}-${arch})${NC}"
echo "${MUTED}Asset: ${NC}${asset}"

BIN_DIR="${INSTALL_ROOT}/bin"
UI_DIR="${INSTALL_ROOT}/ui"
TMP_DIR="${TMPDIR:-/tmp}/cab-install-$$"
mkdir -p "$TMP_DIR" "$BIN_DIR"

archive_path="${TMP_DIR}/${asset}"
if ! curl -fsSL -o "$archive_path" "$url"; then
  echo "${RED}Failed to download ${url}${NC}" >&2
  echo "${MUTED}CLI archives are published as cab-<os>-<arch>.zip / .tar.gz on GitHub Releases.${NC}" >&2
  rm -rf "$TMP_DIR"
  exit 1
fi
if [[ ! -s "$archive_path" ]]; then
  echo "${RED}Downloaded archive is empty: ${url}${NC}" >&2
  rm -rf "$TMP_DIR"
  exit 1
fi

extract_dir="${TMP_DIR}/extract"
mkdir -p "$extract_dir"
if [[ "$archive_ext" == "tar.gz" ]]; then
  tar -xzf "$archive_path" -C "$extract_dir"
else
  unzip -q "$archive_path" -d "$extract_dir"
fi

# Archive layout: cab, ui/ at top level (or nested one directory).
payload="$extract_dir"
if [[ ! -f "${payload}/cab" && ! -f "${payload}/cab.exe" ]]; then
  # single top-level folder
  for d in "$extract_dir"/*; do
    if [[ -d "$d" && ( -f "$d/cab" || -f "$d/cab.exe" ) ]]; then
      payload="$d"
      break
    fi
  done
fi

cab_src="${payload}/cab"
[[ "$os" == "windows" ]] && cab_src="${payload}/cab.exe"

[[ -f "$cab_src" ]] || {
  echo "${RED}Archive missing cab binary${NC}" >&2
  rm -rf "$TMP_DIR"
  exit 1
}

# Replace binaries atomically where possible.
copy_bin() {
  local src=$1 dst=$2
  if command -v install >/dev/null 2>&1; then
    install -m 755 "$src" "$dst"
  else
    cp "$src" "$dst"
    chmod 755 "$dst" 2>/dev/null || true
  fi
}
copy_bin "$cab_src" "${BIN_DIR}/$(basename "$cab_src")"

if [[ -d "${payload}/ui" ]]; then
  rm -rf "$UI_DIR"
  mkdir -p "$(dirname "$UI_DIR")"
  cp -R "${payload}/ui" "$UI_DIR"
fi

rm -rf "$TMP_DIR"

# Persist install metadata for `cab update`.
mkdir -p "$INSTALL_ROOT"
cat >"${INSTALL_ROOT}/install.json" <<EOF
{
  "version": "${version}",
  "os": "${os}",
  "arch": "${arch}",
  "bin_dir": "${BIN_DIR}",
  "ui_dir": "${UI_DIR}",
  "installed_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
EOF

add_to_path() {
  local config_file=$1
  local line=$2
  if [[ ! -f "$config_file" ]]; then
    return 1
  fi
  if grep -Fq "$line" "$config_file" 2>/dev/null; then
    return 0
  fi
  if [[ -w "$config_file" ]]; then
    printf '\n# CAB\n%s\n' "$line" >>"$config_file"
    echo "${MUTED}Added PATH entry to ${NC}${config_file}"
    return 0
  fi
  return 1
}

path_line="export PATH=\"${BIN_DIR}:\$PATH\""
if [[ "$NO_MODIFY_PATH" != "true" ]]; then
  if [[ ":${PATH}:" != *":${BIN_DIR}:"* ]]; then
    shell_name=$(basename "${SHELL:-bash}")
    case "$shell_name" in
      zsh)
        add_to_path "${ZDOTDIR:-$HOME}/.zshrc" "$path_line" \
          || add_to_path "$HOME/.zprofile" "$path_line" || true
        ;;
      fish)
        add_to_path "${XDG_CONFIG_HOME:-$HOME/.config}/fish/config.fish" \
          "fish_add_path ${BIN_DIR}" || true
        ;;
      *)
        add_to_path "$HOME/.bashrc" "$path_line" \
          || add_to_path "$HOME/.bash_profile" "$path_line" \
          || add_to_path "$HOME/.profile" "$path_line" || true
        ;;
    esac
    export PATH="${BIN_DIR}:${PATH}"
  fi
else
  export PATH="${BIN_DIR}:${PATH}"
fi

if [[ -n "${GITHUB_ACTIONS:-}" && "${GITHUB_ACTIONS}" == "true" ]]; then
  echo "$BIN_DIR" >>"${GITHUB_PATH}"
fi

echo ""
echo "${GREEN}CAB ${version} installed to ${BIN_DIR}${NC}"
echo "${MUTED}Binary:${NC} cab"
[[ -d "$UI_DIR" ]] && echo "${MUTED}UI:${NC} ${UI_DIR}"

if [[ "$NO_SERVICE" != "true" ]]; then
  if [[ -x "${BIN_DIR}/cab" || -x "${BIN_DIR}/cab.exe" ]]; then
    echo "${MUTED}Installing user service…${NC}"
    if "${BIN_DIR}/cab" service install --scope user; then
      "${BIN_DIR}/cab" start || true
      echo "${MUTED}Gateway:${NC} http://127.0.0.1:3125"
    else
      echo "${MUTED}Service install skipped/failed — run later:${NC}"
      echo "  cab service install --scope user && cab start"
    fi
  fi
fi

echo ""
echo "${MUTED}Next:${NC}"
echo "  cab status"
echo "  cab gui                 # open dashboard in browser"
echo "  cab update              # upgrade to latest release"
echo "  Docs: https://xiongdi.github.io/cab/"
echo ""
if [[ ":${PATH}:" != *":${BIN_DIR}:"* ]] && [[ "$NO_MODIFY_PATH" == "true" ]]; then
  echo "${MUTED}Add to PATH:${NC} export PATH=\"${BIN_DIR}:\$PATH\""
fi
