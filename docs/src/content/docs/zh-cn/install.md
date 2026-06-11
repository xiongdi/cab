---
title: 安装指南
description: 从 GitHub Releases 下载并安装 CAB 桌面版。
---

本文说明如何从 [GitHub Releases](https://github.com/xiongdi/cab/releases) 下载并安装桌面版。若需从源码构建，请参阅 [GitHub 上的项目 README](https://github.com/xiongdi/cab#%E5%BF%AB%E9%80%9F%E5%BC%80%E5%A7%8B)。

## 系统要求

| 平台        | 最低版本                                      | 架构                            | 说明                                                                                                                                   |
| ----------- | --------------------------------------------- | ------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| **Windows** | Windows 10 **1809** 或更高（推荐 Windows 11） | x64、ARM64                      | 需要 [WebView2 运行时](https://developer.microsoft.com/microsoft-edge/webview2/)。Windows 11 通常已内置；Windows 10 可能需要单独安装。 |
| **macOS**   | **10.13**（High Sierra）或更高                | Intel + Apple Silicon（通用包） | 下载 `.dmg` 通用安装包。                                                                                                               |
| **Linux**   | 支持 **WebKitGTK 4.1** 的发行版               | x64、ARM64                      | 已在 Ubuntu 22.04+、Debian 12+、Fedora 36+ 上验证。                                                                                    |

**无头服务**（`cab-server`）：操作系统要求与上表相同，但不需要 WebView。可用 `cargo run -p cab-server` 运行，或自行编译后部署。

## 如何选择安装包

将下文中的 `VERSION` 替换为发布号（不含 `v` 前缀，例如 `0.2.3`）。

### Windows

| 设备               | 推荐文件                                                       | 说明                          |
| ------------------ | -------------------------------------------------------------- | ----------------------------- |
| 普通 PC（x64）     | `CAB_VERSION_x64_zh-CN.msi` 或 `CAB_VERSION_x64_en-US.msi`     | 中文或英文安装向导的 MSI      |
| 普通 PC（x64）     | `CAB_VERSION_x64-setup.exe`                                    | NSIS 安装包，安装时可选择语言 |
| Surface / ARM 电脑 | `CAB_VERSION_arm64_zh-CN.msi` 或 `CAB_VERSION_arm64_en-US.msi` | ARM64 版 MSI                  |
| Surface / ARM 电脑 | `CAB_VERSION_arm64-setup.exe`                                  | ARM64 版 NSIS                 |

### macOS

| 文件                        | 说明                                          |
| --------------------------- | --------------------------------------------- |
| `CAB_VERSION_universal.dmg` | 拖入「应用程序」即可（Intel + Apple Silicon） |

### Linux

| 发行版                            | 文件                                                           | 安装命令                                |
| --------------------------------- | -------------------------------------------------------------- | --------------------------------------- |
| Debian / Ubuntu（x64）            | `CAB_VERSION_amd64.deb`                                        | `sudo dpkg -i CAB_VERSION_amd64.deb`    |
| Debian / Ubuntu（ARM64）          | `CAB_VERSION_arm64.deb`                                        | `sudo dpkg -i CAB_VERSION_arm64.deb`    |
| Fedora / RHEL / openSUSE（x64）   | `CAB-VERSION-1.x86_64.rpm`                                     | `sudo rpm -i CAB-VERSION-1.x86_64.rpm`  |
| Fedora / RHEL / openSUSE（ARM64） | `CAB-VERSION-1.aarch64.rpm`                                    | `sudo rpm -i CAB-VERSION-1.aarch64.rpm` |
| 通用便携                          | `CAB_VERSION_amd64.AppImage` 或 `CAB_VERSION_aarch64.AppImage` | `chmod +x` 后执行                       |

RPM 文件名中的 `-1` 是软件包 **Release** 号（RPM 惯例），不是应用版本号的一部分。

## 安装后快速开始

1. 从开始菜单 / 应用程序 / 启动器打开 **CAB**。
2. 在 **提供商（Providers）** 中添加至少一个 LLM API Key。
3. 将编码代理指向本地网关：

   ```
   http://127.0.0.1:3125/v1
   ```

4. 在仪表盘 **代理（Agents）** 中将 Agent 配置切换为 CAB（自动或手动模式）。

## 常见问题

| 现象             | 可能原因           | 处理建议                                                                         |
| ---------------- | ------------------ | -------------------------------------------------------------------------------- |
| Windows 窗口空白 | 未安装 WebView2    | 安装 [WebView2 运行时](https://developer.microsoft.com/microsoft-edge/webview2/) |
| macOS 无法打开   | Gatekeeper 拦截    | 右键 → 打开，或在「系统设置 → 隐私与安全性」中允许                               |
| Linux 安装失败   | WebKitGTK 版本过旧 | 升级到带 WebKitGTK 4.1 的发行版，或改用 AppImage                                 |
| Agent 无法连接   | 网关未启动         | 确认 CAB 已运行，且端口 `3125` 未被占用                                          |
