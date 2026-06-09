---
title: 用户需求文件
description: CAB 功能需求的 URD 基线
chapter: requirements
order: 2
---

## URD 功能需求条目（源自已实现能力）

### REQ-CAB-001 多协议代理

系统应透明拦截并转发以下 API：

| 协议               | Gateway 路由                | 处理器         |
| ------------------ | --------------------------- | -------------- |
| OpenAI Chat        | `POST /v1/chat/completions` | `openai.rs`    |
| OpenAI Responses   | `POST /v1/responses`        | `openai.rs`    |
| Anthropic Messages | `POST /v1/messages`         | `anthropic.rs` |

### REQ-CAB-002 路由策略

用户可通过 Agent 模式或 Route 配置选择：`auto`、`balanced`、`cheapest`、`intelligent`（`RoutingStrategy::parse`）。

### REQ-CAB-003 提供商与 Key 管理

- 提供商目录从 models.dev 同步，不可手动增删（`create_provider`/`delete_provider` 返回错误）
- 用户可配置多 Key、启用/禁用、标记订阅（`providers/+page.svelte`）
- 启用提供商须至少有一个已启用非空 Key（`update_provider` 校验）

### REQ-CAB-004 订阅 Key 优先

- 路由成本：订阅提供商边际成本视为 `MIN_COST_EPSILON`（`effective_routing_cost`）
- 429 时记录 `quota_reset_at` 并 fallback（`fallback.rs` + `mark_api_key_quota_reset`）

### REQ-CAB-005 请求日志

每次代理请求记录 agent、provider、model、token、延迟、状态码（`RequestLog` + 各 handler）。

### REQ-CAB-006 智能体集成

支持 7 种预置 Agent 的配置写入与 `native`/`auto`/`manual` 模式（`agents.rs`）。

## 验收映射

每条 REQ 须在验收章有对应 UAT 用例。
