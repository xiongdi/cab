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

- `gateway_key` 在首次安装时生成，保存在 SQLite `~/.cab/cab.db`（`settings` 表，`id = 1`）。
- 在 **设置 → Gateway API Key** 查看或重新生成。
- 通过 CAB 配置的 Agent 会自动获得该密钥。
- 外部客户端需手动添加请求头。

可在设置中关闭 `auth_enabled`，但建议保持开启以确保本地安全。

## 配置存储

| 位置               | 内容                                                      |
| ------------------ | --------------------------------------------------------- |
| `~/.cab/cab.db`    | 设置（端口、网关密钥、认证）、Agent、路由、请求日志等     |
| `cab.toml`         | 系统引导：host + 首次安装端口种子（不可通过 API 修改）    |
| `~/.cab/catalog/`  | models.dev 等下载缓存                                     |

已废弃（勿作运行时配置）：`~/.cab/settings.json`、`~/.cab/state.json`、`~/.cab/logs/*.jsonl`。

## 修改端口

默认端口 **3125**。在设置中修改后需重启 CAB，并更新 Agent 配置中的端点。

## 协议转换

当模型原生协议与 Agent 请求协议不一致时（如通过 OpenAI 协议调用仅支持 Anthropic 的模型），CAB 在网关层自动转换并转发到最佳匹配端点。

## 无头服务

无需桌面 UI 时（用于发布测试或生产环境）：

```bash
cargo run -p cab-srv
```

无头守护进程提供相同的网关和管理 API，同时提供内置 UI 的静态文件服务。

> 日常开发请使用 `npm run dev:server`（cargo watch 热重载），参见 [AGENTS.md](https://github.com/xiongdi/cab/blob/main/AGENTS.md)。

## 相关

- [API 参考](../../reference/api/)
- [系统架构](../../reference/architecture/)
