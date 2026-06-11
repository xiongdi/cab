#!/usr/bin/env bash
# Generate GitHub Release notes from CHANGELOG.md for a given tag (e.g. v0.1.2).
set -euo pipefail

TAG="${1:?usage: generate-release-body.sh <tag>  (e.g. v0.1.2)}"
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

- Desktop installers for Windows, macOS, and Linux (x64 + ARM64 where applicable).
- Local LLM gateway for coding agents at \`http://127.0.0.1:3125/v1\`.
- Full install guide: [English](https://xiongdi.github.io/cab/install/) · [简体中文](https://xiongdi.github.io/cab/zh-cn/install/)

---

## Changelog / 变更记录

${CHANGELOG_SECTION}

---

## System requirements / 系统要求

| Platform | Minimum | Architectures | Notes |
| -------- | ------- | ------------- | ----- |
| **Windows** | Windows 10 **1809+** (Win 11 recommended) | x64, ARM64 | [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) required |
| **macOS** | **10.13+** | Intel + Apple Silicon | Universal \`.dmg\` |
| **Linux** | WebKitGTK **4.1+** | x64, ARM64 | e.g. Ubuntu 22.04+, Debian 12+, Fedora 36+ |

| 平台 | 最低版本 | 架构 | 说明 |
| ---- | -------- | ---- | ---- |
| **Windows** | Windows 10 **1809+**（推荐 Win 11） | x64、ARM64 | 需 [WebView2 运行时](https://developer.microsoft.com/microsoft-edge/webview2/) |
| **macOS** | **10.13+** | Intel + Apple Silicon | 通用 \`.dmg\` |
| **Linux** | WebKitGTK **4.1+** | x64、ARM64 | 如 Ubuntu 22.04+、Debian 12+、Fedora 36+ |

---

## Download & install / 下载与安装

### Windows

| Device | File | Notes |
| ------ | ---- | ----- |
| PC x64 | \`CAB_${VERSION}_x64_zh-CN.msi\` | Chinese installer UI |
| PC x64 | \`CAB_${VERSION}_x64_en-US.msi\` | English installer UI |
| PC x64 | \`CAB_${VERSION}_x64-setup.exe\` | NSIS — language selector at install |
| ARM PC | \`CAB_${VERSION}_arm64_zh-CN.msi\` | ARM64, Chinese UI |
| ARM PC | \`CAB_${VERSION}_arm64_en-US.msi\` | ARM64, English UI |
| ARM PC | \`CAB_${VERSION}_arm64-setup.exe\` | ARM64 NSIS |

### macOS

| File | Notes |
| ---- | ----- |
| \`CAB_${VERSION}_universal.dmg\` | Universal (Intel + Apple Silicon) |

### Linux

| Distro | File | Install |
| ------ | ---- | ------- |
| Debian/Ubuntu x64 | \`CAB_${VERSION}_amd64.deb\` | \`sudo dpkg -i CAB_${VERSION}_amd64.deb\` |
| Debian/Ubuntu ARM64 | \`CAB_${VERSION}_arm64.deb\` | \`sudo dpkg -i CAB_${VERSION}_arm64.deb\` |
| Fedora/RHEL x64 | \`CAB-${VERSION}-1.x86_64.rpm\` | \`sudo rpm -i CAB-${VERSION}-1.x86_64.rpm\` |
| Fedora/RHEL ARM64 | \`CAB-${VERSION}-1.aarch64.rpm\` | \`sudo rpm -i CAB-${VERSION}-1.aarch64.rpm\` |
| Portable | \`CAB_${VERSION}_amd64.AppImage\` / \`CAB_${VERSION}_aarch64.AppImage\` | \`chmod +x\` then run |

RPM 文件名中的 \`-1\` 为 Release 号，非应用版本。

---

## Quick start / 快速开始

1. Install and launch **CAB** / 安装并启动 **CAB**
2. Add LLM API keys under **Providers** / 在 **提供商** 中添加 API Key
3. Point your agent to \`http://127.0.0.1:3125/v1\` / 将代理指向 \`http://127.0.0.1:3125/v1\`
4. Use **Agents** to switch agent configs / 在 **代理** 中切换 Agent 配置
EOF

echo "Wrote $OUTPUT"
