---
title: API 参考
description: CAB 网关与管理 API 概览。
---

CAB 提供两个 API 面：**网关**（OpenAI/Anthropic 兼容，供 Agent 使用）和 **管理 API**（REST，供仪表盘使用）。

## 网关 API

基础地址：`http://127.0.0.1:3125/v1`

使用 `Authorization: Bearer <gateway_key>` 认证（亦接受 `x-api-key`）。

| 方法   | 路径                   | 说明                |
| ------ | ---------------------- | ------------------- |
| `POST` | `/v1/chat/completions` | OpenAI 对话补全     |
| `POST` | `/v1/messages`         | Anthropic 消息      |
| `POST` | `/v1/responses`        | OpenAI Responses    |
| `GET`  | `/v1/responses`        | Responses WebSocket |
| `GET`  | `/v1/models`           | 列出可路由模型      |

Agent 通过 User-Agent 标识自身，CAB 据此匹配路由。

## 管理 API

基础地址：`http://127.0.0.1:3125/api`

`auth_enabled` 为 true 时同样需要 Bearer 认证（本机仪表盘 Origin/Referer 可绕过）。

| 领域           | 端点                                                | 用途                            |
| -------------- | --------------------------------------------------- | ------------------------------- |
| **设置**       | `GET/PUT /api/settings`                             | 端口、网关密钥、认证、日志保留  |
| **设置**       | `GET /api/settings/catalog-status`                  | 目录同步状态                    |
| **设置**       | `POST /api/settings/sync-catalog`                   | 触发目录同步                    |
| **提供商**     | `/api/providers/*`                                  | 提供商目录与 Key 管理           |
| **模型**       | `/api/models/*`、`PUT /api/model-endpoints`         | 模型目录、可路由/目录列表、端点 |
| **路由**       | `/api/routes/*`                                     | 自定义路由规则                  |
| **Agent**      | `/api/agents/*`                                     | Agent 模式与策略配置            |
| **日志**       | `GET/DELETE /api/logs`                              | 请求日志查询 / 清空             |
| **用量**       | `GET /api/usage/summary`、`/api/usage/records`      | 用量汇总与明细                  |
| **路由解释**   | `POST /api/routing/explain`                         | 预览提示词的路由决策            |
| **策略排序板** | `POST /api/routing/strategy-board`                  | 各内置策略的完整候选排序        |
| **诊断**       | `GET /api/diagnostics/tool-weights`                 | 工具权重诊断                    |
| **仪表盘**     | `GET /api/dashboard/stats`                          | 统计与健康状态                  |
| **更新**       | `GET /api/update/check`、`POST /api/update/install` | 应用更新检查 / 安装             |
| **Logo**       | `GET /api/logos/{*path}`                            | 提供商 Logo                     |

仓库中维护 OpenAPI 规范（`spec/`），可通过项目脚本生成前端类型。

## 路由解释

`POST /api/routing/explain` 接受 Agent ID、可选模型/策略和示例消息，返回：

- 解析结果（提供商 + 模型）
- 决策步骤
- 候选排序列表

对应仪表盘 **路由 → 解释路由** 预览功能。

## 策略排序板

`POST /api/routing/strategy-board` 接受 Agent ID 与示例消息体，返回六种内置策略（`auto`、`balanced`、`cheapest`、`intelligent`、`speed`、`agentic`）的完整候选排序列表。排序算法与网关 `cab-core::routing` 一致；仪表盘 **路由** 页各策略候选表直接消费此接口，不在前端重复实现。

## 相关

- [网关与认证](../../guides/gateway-auth/)
- [GitHub 仓库](https://github.com/xiongdi/cab)
