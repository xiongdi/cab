---
title: 安装
description: 在 Windows、macOS 和 Linux 上下载并安装 CAB。
---

从 [GitHub Releases](https://github.com/xiongdi/cab/releases) 下载预编译桌面安装包。若需从源码构建，请参阅 [仓库 README](https://github.com/xiongdi/cab#%E5%BF%AB%E9%80%9F%E5%BC%80%E5%A7%8B)。

## 系统要求

| 平台 | 最低版本 | 架构 | 说明 |
| ---- | -------- | ---- | ---- |
| **Windows** | Windows 10 1809+（推荐 11） | x64、ARM64 | 需要 [WebView2 运行时](https://developer.microsoft.com/microsoft-edge/webview2/) |
| **macOS** | 10.13 High Sierra+ | Intel + Apple Silicon | 通用 `.dmg` 安装包 |
| **Linux** | WebKitGTK 4.1 | x64、ARM64 | 已在 Ubuntu 22.04+、Debian 12+、Fedora 36+ 验证 |

**无头服务**（`cab-server`）可在相同操作系统上运行，无需 WebView。使用 `cargo run -p cab-server` 或自行编译部署。

## 选择安装包

将 `VERSION` 替换为发布号（不含 `v` 前缀，如 `0.2.3`）。

### Windows

| 设备 | 文件 | 说明 |
| ---- | ---- | ---- |
| 普通 PC（x64） | `CAB_VERSION_x64_zh-CN.msi` 或 `CAB_VERSION_x64_en-US.msi` | MSI 安装包 |
| 普通 PC（x64） | `CAB_VERSION_x64-setup.exe` | NSIS，可选语言 |
| ARM 电脑 | `CAB_VERSION_arm64_zh-CN.msi` 或 `CAB_VERSION_arm64-setup.exe` | ARM64 版本 |

### macOS

| 文件 | 说明 |
| ---- | ---- |
| `CAB_VERSION_universal.dmg` | 拖入「应用程序」 |

### Linux

| 发行版 | 文件 | 安装 |
| ------ | ---- | ---- |
| Debian / Ubuntu | `CAB_VERSION_amd64.deb` | `sudo dpkg -i …` |
| Fedora / RHEL | `CAB-VERSION-1.x86_64.rpm` | `sudo rpm -i …` |
| 便携版 | `CAB_VERSION_amd64.AppImage` | `chmod +x` 后执行 |

## 安装后

1. 从启动器打开 **CAB**。
2. 继续阅读 [快速开始](../quick-start/)。
3. Windows 窗口空白请安装 [WebView2](https://developer.microsoft.com/microsoft-edge/webview2/)；macOS 被拦截请右键 → 打开。

## 常见问题

| 现象 | 处理 |
| ---- | ---- |
| Windows 窗口空白 | 安装 WebView2 运行时 |
| macOS 无法打开 | 右键 → 打开，或在系统设置中允许 |
| Linux 安装失败 | 升级到 WebKitGTK 4.1，或改用 AppImage |
| Agent 无法连接 | 确认 CAB 已运行，端口 `3125` 未被占用 |
