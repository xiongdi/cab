---
title: 网关与认证
description: CAB 网关端点、认证与本地配置。
---

CAB 暴露兼容 OpenAI 和 Anthropic 客户端 SDK 的本地 HTTP 网关，以及供仪表盘使用的管理 API。

## 网关端点

默认基础 URL：

```
http://127.0.0.1:3125/v1
```

| 端点                        | 协议      | 用途                       |
| --------------------------- | --------- | -------------------------- |
| `POST /v1/chat/completions` | OpenAI    | 对话补全（多数 Agent）     |
| `POST /v1/messages`         | Anthropic | Anthropic Messages API     |
| `POST /v1/responses`        | OpenAI    | Responses API              |
| `GET /v1/responses`         | OpenAI    | Responses WebSocket        |
| `GET /v1/models`            | OpenAI    | 列出可路由模型（手动模式） |

CAB 从 User-Agent 识别调用 Agent 并应用对应路由或策略。

## 认证

默认 **开启网关认证**：

```
Authorization: Bearer <gateway_key>
```

网关亦接受 `x-api-key: <gateway_key>`（两者同时存在时以 Bearer 为准）。

- `gateway_key` 在首次安装时生成，保存在 SQLite `$CAB_HOME/cab.db`（默认 `~/.cab/cab.db`，`settings` 表，`id = 1`）。
- 在 **设置 → Gateway API Key** 查看或重新生成。
- 通过 CAB 配置的 Agent 会自动获得该密钥。
- 外部客户端需手动添加请求头。

可在设置中关闭 `auth_enabled`，但建议保持开启以确保本地安全。

## 配置存储

| 位置                                       | 内容                                                   |
| ------------------------------------------ | ------------------------------------------------------ |
| `$CAB_HOME/cab.db`（默认 `~/.cab/cab.db`） | 设置（端口、网关密钥、认证）、Agent、路由、请求日志等  |
| `$CAB_HOME/service.json`                   | 已安装服务范围（`user` / `system`）                    |
| `cab.toml`                                 | 系统引导：host + 首次安装端口种子（不可通过 API 修改） |
| `$CAB_HOME/catalog/`                       | models.dev 等下载缓存                                  |

已废弃（勿作运行时配置）：`~/.cab/settings.json`、`~/.cab/state.json`、`~/.cab/logs/*.jsonl`。

## 修改端口

默认端口 **3125**。在设置中修改后需重启 CAB，并更新 Agent 配置中的端点。

## 协议转换

当模型原生协议与 Agent 请求协议不一致时（如通过 OpenAI 协议调用仅支持 Anthropic 的模型），CAB 在网关层自动转换并转发到最佳匹配端点。

## 无头服务 / 守护进程

`cab-srv` 是**唯一** HTTP 服务（网关 + API + 静态 UI）。可按 **用户级** 或 **系统级** 安装为后台服务：

```bash
cab-cli service install --scope user    # 默认：登录后运行，数据在 ~/.cab
sudo cab-cli service install --scope system  # 开机自启；需管理员/root
cab-cli start
```

| Scope    | 何时运行                       | 数据目录                                                                                    | 权限 / 账户      |
| -------- | ------------------------------ | ------------------------------------------------------------------------------------------- | ---------------- |
| `user`   | 用户登录后（默认）             | `~/.cab`                                                                                    | 普通用户         |
| `system` | 开机（登录前也可，视平台而定） | Linux `/var/lib/cab`；macOS `/Library/Application Support/cab`；Windows `%ProgramData%\cab` | 专用最小权限账户 |

平台机制（已加固）：

| 平台    | user                        | system                                                                 |
| ------- | --------------------------- | ---------------------------------------------------------------------- |
| Linux   | `systemd --user` + linger   | 系统 unit，用户 `cab`，`ProtectSystem=strict` 等                       |
| macOS   | LaunchAgent                 | LaunchDaemon，尽量以 `_cab` 运行                                       |
| Windows | 计划任务 ONLOGON + 失败重启 | SCM 服务，账户 `LocalService`，环境变量写在服务注册表（非机级 `setx`） |

范围写入数据目录下的 `service.json`（用户家目录另有指针便于发现）。可用 `CAB_HOME` 覆盖数据根目录。

`cab-gui` 为薄客户端：首次启动若尚未安装服务会弹出 scope 选择，再启动 `cab-srv` 并打开 `http://127.0.0.1:{port}/`；关闭 GUI 后 daemon 继续常驻。

勿将网关绑到公网 `0.0.0.0`（默认 host 仍来自 `cab.toml` 的 `127.0.0.1`）。

> 日常开发请使用 `npm run dev:server`（cargo watch 热重载），参见 [AGENTS.md](https://github.com/xiongdi/cab/blob/main/AGENTS.md)。

## 相关

- [API 参考](../../reference/api/)
- [系统架构](../../reference/architecture/)
