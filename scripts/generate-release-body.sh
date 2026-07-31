#!/usr/bin/env bash
# Generate GitHub Release notes from CHANGELOG.md for a given tag (e.g. v0.9.0).
set -euo pipefail

TAG="${1:?usage: generate-release-body.sh <tag>  (e.g. v0.9.0)}"
VERSION="${TAG#v}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUTPUT="${ROOT}/.github/release-body.md"
CHANGELOG="${ROOT}/CHANGELOG.md"

if [[ ! -f "$CHANGELOG" ]]; then
  echo "missing $CHANGELOG" >&2
  exit 1
fi

extract_changelog() {
  awk -v ver="$VERSION" '
    /^## \[/ {
      if (found) exit
      if ($0 ~ "\\[" ver "\\]") {
        found = 1
        print
        next
      }
      next
    }
    found { print }
  ' "$CHANGELOG" | sed '/^$/N;/^\n$/d'
}

CHANGELOG_SECTION="$(extract_changelog)"
if [[ -z "$CHANGELOG_SECTION" ]]; then
  echo "no changelog entry for version $VERSION in $CHANGELOG" >&2
  exit 1
fi

mkdir -p "$(dirname "$OUTPUT")"

cat >"$OUTPUT" <<EOF
## Highlights / 亮点

- Single \`cab\` binary: gateway + API + dashboard UI (browser).
- Local LLM gateway for coding agents at \`http://127.0.0.1:3125/v1\`.
- Official site: [English](https://xiongdi.github.io/cab/) · [简体中文](https://xiongdi.github.io/cab/zh-cn/)

---

## Changelog / 变更记录

${CHANGELOG_SECTION}

---

## Download & install / 下载与安装

### One-line install (Linux / macOS / Git Bash)

\`\`\`bash
curl -fsSL https://raw.githubusercontent.com/xiongdi/cab/main/scripts/install.sh | bash
cab gui
cab update
\`\`\`

Archives on this release: \`cab-linux-x64.tar.gz\`, \`cab-linux-arm64.tar.gz\`, \`cab-darwin-x64.tar.gz\`, \`cab-darwin-arm64.tar.gz\`, \`cab-windows-x64.zip\`, \`cab-windows-arm64.zip\`.

Linux archives are **fully static (musl)** and do not require a particular glibc version.

Desktop \`.dmg\` / \`.msi\` / AppImage installers are no longer shipped (use the CLI + browser dashboard).

---

## Quick start / 快速开始

1. Install with the curl installer / 用 curl 安装
2. Open the dashboard: \`cab gui\` / 打开仪表盘
3. Add LLM API keys under **Providers** / 在 **提供商** 中添加 API Key
4. Point your agent to \`http://127.0.0.1:3125/v1\` / 将代理指向 \`http://127.0.0.1:3125/v1\`
EOF

echo "Wrote $OUTPUT"
