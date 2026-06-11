---
title: API 参考
description: CAB 网关与管理 API 概览。
---

CAB 提供两个 API 面：**网关**（OpenAI/Anthropic 兼容，供 Agent 使用）和 **管理 API**（REST，供仪表盘使用）。

## 网关 API

基础地址：`http://127.0.0.1:3125/v1`

使用 `Authorization: Bearer <gateway_key>` 认证。

| 方法 | 路径 | 说明 |
| ---- | ---- | ---- |
| `POST` | `/v1/chat/completions` | OpenAI 对话补全 |
| `POST` | `/v1/messages` | Anthropic 消息 |
| `POST` | `/v1/responses` | OpenAI Responses |
| `GET` | `/v1/models` | 列出可路由模型 |

Agent 通过 User-Agent 标识自身，CAB 据此匹配路由。

## 管理 API

基础地址：`http://127.0.0.1:3125/api`

`auth_enabled` 为 true 时同样需要 Bearer 认证。

| 领域 | 端点 | 用途 |
| ---- | ---- | ---- |
| **设置** | `GET/PUT /api/settings` | 端口、网关密钥、认证、日志保留 |
| **提供商** | `/api/providers/*` | 提供商目录与 Key 管理 |
| **模型** | `/api/models/*` | 模型目录、启用/禁用 |
| **路由** | `/api/routes/*` | 自定义路由规则 |
| **Agent** | `/api/agents/*` | Agent 模式与策略配置 |
| **日志** | `/api/logs/*` | 请求日志查询 |
| **路由解释** | `POST /api/routing/explain` | 预览提示词的路由决策 |
| **仪表盘** | `/api/dashboard/*` | 统计与健康状态 |

仓库中维护 OpenAPI 规范（`spec/`），可通过项目脚本生成前端类型。

## 路由解释

`POST /api/routing/explain` 接受 Agent ID、可选模型/策略和示例消息，返回：

- 解析结果（提供商 + 模型）
- 决策步骤
- 候选排序列表

对应仪表盘 **路由 → 解释路由** 预览功能。

## 相关

- [网关与认证](../../guides/gateway-auth/)
- [GitHub 仓库](https://github.com/xiongdi/cab)
