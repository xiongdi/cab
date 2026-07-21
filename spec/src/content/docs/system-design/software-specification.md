---
title: 软件规格说明书
description: CAB 系统高层规格，对应系统测试
chapter: system-design
order: 1
---

## 系统概述

CAB 是本地 LLM 网关：管理面（Svelte + `/api`）与数据面（Gateway `/v1`）共存于同一 Axum 进程（`cab-srv` 或 `src-tauri` 内嵌）。

## 核心处理流程

```
Agent SDK → HTTP :{gateway_port}（默认 3125）→ cab-gateway
  → resolve_route(agent, model, body)
  → rank_models / 具体模型
  → execute_with_fallback(keys × endpoints × models)
  → proxy_request → 上游 LLM
  → 协议转换 → 响应 Agent
  → 异步写入 RequestLog
```

## 软件规格要点

### 路由引擎（`cab-core`）

- 输入：`RequestProfile`（任务类型 + 复杂度 + 估计 token）
- 成本公式：`effective_token_cost = blended_input×ratio + output`（默认 `BALANCED_INPUT_OUTPUT_RATIO = 10`；balanced/auto 可用请求画像比例）
- Auto 策略按复杂度动态提高最低能力门槛（`min_required_capability`），低复杂度时另有成本上限

### 数据持久化

- 运行时：`InMemoryStore`（`providers`、`models`、`routes`、`agents`、`request_logs`、`model_endpoints`、`settings`）
- 磁盘：`~/.cab/cab.db`（SQLite）与可选 `~/.cab/catalog/` 缓存

### 目录三源合并（Models 页）

`GET /api/models/catalog` 返回：

- `models_dev`：目录原始 JSON
- `artificial_analysis`：AA 基准记录
- `settings`：用户 `enabled` 覆盖

## 系统测试计划入口

见 `system-design/system-test-plan.md`；验证上述端到端流程。
