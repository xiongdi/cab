---
title: 安装
description: 在 Windows、macOS 与 Linux 上下载并安装 CAB。
---

## 一键安装

从 GitHub Releases 安装单一 `cab` 二进制（含仪表盘 UI）：

**Linux / macOS / Git Bash：**

```bash
curl -fsSL https://raw.githubusercontent.com/xiongdi/cab/main/scripts/install.sh | bash
# 镜像: curl -fsSL https://xiongdi.github.io/cab/install.sh | bash
```

**Windows（PowerShell）：**

```powershell
irm https://raw.githubusercontent.com/xiongdi/cab/main/scripts/install.ps1 | iex
# 镜像: irm https://xiongdi.github.io/cab/install.ps1 | iex
```

默认安装到 `%USERPROFILE%\.cab\bin`（Windows）或 `~/.cab/bin`，写入 `PATH`，并执行 `cab service install --scope user`。

```bash
cab status
cab gui                     # 在浏览器中打开仪表盘
cab update                  # 升级到最新版
cab update --check          # 仅检查是否有新版本
cab update --version 0.9.0
```

安装器选项：

```bash
curl -fsSL …/install.sh | bash -s -- --version 0.9.0
curl -fsSL …/install.sh | bash -s -- --no-service --no-modify-path
```

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File install.ps1 -Version 0.9.0
powershell -NoProfile -ExecutionPolicy Bypass -File install.ps1 -NoService -NoModifyPath
```

自 **0.9** 起不再提供桌面 `.dmg` / `.msi` / AppImage。请用 CLI，并通过 `cab gui` 打开 UI。从源码构建见 [仓库 README](https://github.com/xiongdi/cab#getting-started)。

## 系统要求

| 平台        | 最低版本        | 架构                                  | 说明                         |
| ----------- | --------------- | ------------------------------------- | ---------------------------- |
| **Windows** | Windows 7+      | x64、ARM64                            | 无需 WebView；仪表盘在浏览器 |
| **macOS**   | 10.15 Catalina+ | Intel (x86_64)、Apple Silicon (arm64) | 按架构提供独立归档           |
| **Linux**   | glibc 2.35+     | x64、ARM64                            | 在 Ubuntu 22.04 构建         |

发布测试可用 `cargo run -p cab --bin cab -- serve`，或使用 GitHub Releases 预编译包。日常开发请遵循 [AGENTS.md](https://github.com/xiongdi/cab/blob/main/AGENTS.md) 双终端流程。

## 发布归档

将 `VERSION` 换成不含 `v` 的版本号（如 `0.9.0`）。每个 Release 提供：

| 平台                        | 文件                                                |
| --------------------------- | --------------------------------------------------- |
| Linux x64 / ARM64           | `cab-linux-x64.tar.gz` / `cab-linux-arm64.tar.gz`   |
| macOS Intel / Apple Silicon | `cab-darwin-x64.tar.gz` / `cab-darwin-arm64.tar.gz` |
| Windows x64 / ARM64         | `cab-windows-x64.zip` / `cab-windows-arm64.zip`     |

归档内容为 `cab`（或 `cab.exe`）与 `ui/` 目录。

## 安装之后

1. 运行 `cab gui`（必要时启动服务并打开仪表盘）。
2. 若跳过了服务安装，可选择 **service scope**：
   - **当前用户**（默认）— 数据在 `~/.cab`；登录后启动。
   - **系统** — 数据在 `/var/lib/cab`、`/Library/Application Support/cab` 或 `%ProgramData%\cab`；需管理员；开机启动。
3. 继续阅读 [快速开始](../quick-start/)。

```bash
cab service install --scope user   # 或：--scope system（需提权）
cab start
cab status
cab gui
```

## 故障排除

| 现象         | 处理                                                    |
| ------------ | ------------------------------------------------------- |
| 浏览器未打开 | 手动访问 `http://127.0.0.1:3125/`                       |
| Agent 连不上 | 确认 CAB 在跑且端口 `3125` 空闲                         |
| 从 0.8 升级  | 先卸载旧 `cab-cli`/`cab-srv` 服务，再用 curl 安装器重装 |
