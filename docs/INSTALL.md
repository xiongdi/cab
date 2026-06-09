# CAB Installation Guide

[English](INSTALL.md) | [简体中文](INSTALL.zh-CN.md)

This guide covers desktop installer downloads from [GitHub Releases](https://github.com/xiongdi/cab/releases). For building from source, see the [README](../README.md).

---

## System requirements

| Platform    | Minimum version                                       | Architectures                           | Notes                                                                                                                                                       |
| ----------- | ----------------------------------------------------- | --------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Windows** | Windows 10 **1809** or later (Windows 11 recommended) | x64, ARM64                              | Requires [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/). Pre-installed on Windows 11; Windows 10 may need a separate install. |
| **macOS**   | **10.13** (High Sierra) or later                      | Intel + Apple Silicon (universal build) | Download the `.dmg` universal package.                                                                                                                      |
| **Linux**   | Distro with **WebKitGTK 4.1**                         | x64, ARM64                              | Tested on Ubuntu 22.04+, Debian 12+, Fedora 36+.                                                                                                            |

**Headless server** (`cab-server`): same OS families; no WebView requirement. Run with `cargo run -p cab-server` or ship the binary from your own build.

---

## Choose the right download

Replace `VERSION` with the release tag without the `v` prefix (e.g. `0.1.2`).

### Windows

| Device           | Recommended file                                               | Notes                                    |
| ---------------- | -------------------------------------------------------------- | ---------------------------------------- |
| PC (x64)         | `CAB_VERSION_x64_zh-CN.msi` or `CAB_VERSION_x64_en-US.msi`     | MSI with Chinese or English installer UI |
| PC (x64)         | `CAB_VERSION_x64-setup.exe`                                    | NSIS; pick language at install time      |
| Surface / ARM PC | `CAB_VERSION_arm64_zh-CN.msi` or `CAB_VERSION_arm64_en-US.msi` | ARM64 MSI                                |
| Surface / ARM PC | `CAB_VERSION_arm64-setup.exe`                                  | ARM64 NSIS                               |

### macOS

| File                        | Notes                                              |
| --------------------------- | -------------------------------------------------- |
| `CAB_VERSION_universal.dmg` | Drag CAB into Applications (Intel + Apple Silicon) |

### Linux

| Distro family                    | File                                                           | Install                                 |
| -------------------------------- | -------------------------------------------------------------- | --------------------------------------- |
| Debian / Ubuntu (x64)            | `CAB_VERSION_amd64.deb`                                        | `sudo dpkg -i CAB_VERSION_amd64.deb`    |
| Debian / Ubuntu (ARM64)          | `CAB_VERSION_arm64.deb`                                        | `sudo dpkg -i CAB_VERSION_arm64.deb`    |
| Fedora / RHEL / openSUSE (x64)   | `CAB-VERSION-1.x86_64.rpm`                                     | `sudo rpm -i CAB-VERSION-1.x86_64.rpm`  |
| Fedora / RHEL / openSUSE (ARM64) | `CAB-VERSION-1.aarch64.rpm`                                    | `sudo rpm -i CAB-VERSION-1.aarch64.rpm` |
| Any (portable)                   | `CAB_VERSION_amd64.AppImage` or `CAB_VERSION_aarch64.AppImage` | `chmod +x` then run                     |

The `-1` in RPM filenames is the package **release** number (standard RPM convention), not part of the app version.

---

## Quick start after install

1. Launch **CAB** from the Start menu / Applications / app launcher.
2. Open **Providers** and add at least one LLM API key.
3. Point your coding agent at the local gateway:

   ```
   http://127.0.0.1:3125/v1
   ```

4. Use **Agents** in the dashboard to switch agent configs to CAB (Auto or Manual mode).

---

## Troubleshooting

| Symptom                     | Likely cause        | What to try                                                                          |
| --------------------------- | ------------------- | ------------------------------------------------------------------------------------ |
| Blank window on Windows     | Missing WebView2    | Install [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) |
| App won't open on macOS     | Gatekeeper          | Right-click → Open, or allow in System Settings → Privacy & Security                 |
| Linux package install fails | Old WebKitGTK       | Upgrade to a distro with WebKitGTK 4.1, or use AppImage                              |
| Agent can't connect         | Gateway not running | Ensure CAB is running; check port `3125` is free                                     |
