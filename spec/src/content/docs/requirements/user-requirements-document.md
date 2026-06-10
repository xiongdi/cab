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

### REQ-CAB-007 配置持久化

- Agent 模式与 Route 规则存于 `~/.cab/state.json`，进程重启后自动加载
- 首次启动若文件不存在，从内置默认值写出初始 `state.json`
- 写盘采用原子替换（`.tmp` → `rename`）

### REQ-CAB-008 Gateway/API 鉴权

- `settings.auth_enabled` 默认 `true`
- 全部 `/v1/*` 与 `/api/*` 请求须携带 `Authorization: Bearer {gateway_key}`
- 首次创建 `settings.json` 时生成随机 `gateway_key`（非硬编码默认值）
- Agent 配置改写时自动写入 CAB gateway_key

### REQ-CAB-009 日志 JSONL 持久化

- 请求日志追加写入 `~/.cab/logs/requests-YYYY-MM-DD.jsonl`
- 启动时按 `log_retention_days` 删除过期文件
- 日志查询 API 兼容现有分页/筛选语义

### REQ-CAB-010 路由解释

- 提供 `POST /api/routing/explain`，输入 agent/model/body，返回决策链与候选排序
- Routes 管理页提供「模拟请求」UI 调用该 API

## 验收映射

| REQ         | UAT    |
| ----------- | ------ |
| REQ-CAB-001 | UAT-01 |
| REQ-CAB-002 | UAT-02 |
| REQ-CAB-003 | UAT-03 |
| REQ-CAB-004 | UAT-04 |
| REQ-CAB-005 | UAT-05 |
| REQ-CAB-006 | UAT-06 |
| REQ-CAB-007 | UAT-09 |
| REQ-CAB-008 | UAT-10 |
| REQ-CAB-009 | UAT-11 |
| REQ-CAB-010 | UAT-12 |
