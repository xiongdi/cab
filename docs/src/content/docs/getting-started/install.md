---
title: Install
description: Download and install CAB for Windows, macOS, and Linux.
---

Download pre-built desktop installers from [GitHub Releases](https://github.com/xiongdi/cab/releases). To build from source, see the [repository README](https://github.com/xiongdi/cab#getting-started).

## System requirements

| Platform | Minimum version | Architectures | Notes |
| -------- | --------------- | ------------- | ----- |
| **Windows** | Windows 10 1809+ (11 recommended) | x64, ARM64 | Requires [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) |
| **macOS** | 10.13 High Sierra+ | Intel + Apple Silicon | Universal `.dmg` package |
| **Linux** | WebKitGTK 4.1 | x64, ARM64 | Ubuntu 22.04+, Debian 12+, Fedora 36+ tested |

**Headless server** (`cab-server`) runs on the same OS families without WebView. Build from source with `cargo run -p cab-server` for release testing, or use a pre-built binary from GitHub Releases. For daily development, follow the two-terminal workflow in [AGENTS.md](https://github.com/xiongdi/cab/blob/main/AGENTS.md) instead.

## Choose the right package

Replace `VERSION` with the release number without the `v` prefix (e.g. `0.2.3`).

### Windows

| Device | File | Notes |
| ------ | ---- | ----- |
| PC (x64) | `CAB_VERSION_x64_en-US.msi` or `CAB_VERSION_x64_zh-CN.msi` | MSI installer |
| PC (x64) | `CAB_VERSION_x64-setup.exe` | NSIS, language picker |
| ARM PC | `CAB_VERSION_arm64_en-US.msi` or `CAB_VERSION_arm64-setup.exe` | ARM64 builds |

### macOS

| File | Notes |
| ---- | ----- |
| `CAB_VERSION_universal.dmg` | Drag CAB into Applications |

### Linux

| Distro | File | Install |
| ------ | ---- | ------- |
| Debian / Ubuntu | `CAB_VERSION_amd64.deb` | `sudo dpkg -i …` |
| Fedora / RHEL | `CAB-VERSION-1.x86_64.rpm` | `sudo rpm -i …` |
| Portable | `CAB_VERSION_amd64.AppImage` | `chmod +x` then run |

## After install

1. Launch **CAB** from your app launcher.
2. Continue with the [Quick Start](../quick-start/) guide.
3. If the window is blank on Windows, install [WebView2](https://developer.microsoft.com/microsoft-edge/webview2/). On macOS, use Right-click → Open if Gatekeeper blocks the app.

## Troubleshooting

| Symptom | Fix |
| ------- | --- |
| Blank window (Windows) | Install WebView2 Runtime |
| App blocked (macOS) | Right-click → Open, or allow in System Settings |
| Linux install fails | Upgrade to WebKitGTK 4.1, or use AppImage |
| Agent can't connect | Ensure CAB is running; port `3125` is free |
