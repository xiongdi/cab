---
title: 数据库表设计
description: CAB SQLite 模式（~/.cab/cab.db）
chapter: modules
order: 3
---

CAB 以 **SQLite** 为唯一运行时配置与状态存储，路径为 `~/.cab/cab.db`（`SCHEMA_VERSION = 4`）。内存侧为 `InMemoryStore`，启动时从 SQLite 水合，写回经 `cab-db` 持久化。

已废弃（勿作运行时配置）：`~/.cab/settings.json`、`~/.cab/state.json`、`~/.cab/logs/*.jsonl`。

权威 DDL 见 `crates/cab-db/src/sqlite.rs`（`init_schema`）。

## schema_version

| 字段    | 类型    | 说明        |
| ------- | ------- | ----------- |
| version | INTEGER | 当前 schema |

## settings

单行表（`id = 1`），`data` 为 JSON 文本：

| 字段 | 类型    | 约束               |
| ---- | ------- | ------------------ |
| id   | INTEGER | PK，`CHECK (id=1)` |
| data | TEXT    | JSON blob          |

典型 JSON 字段：`gateway_port`、`gateway_key`、`auth_enabled`、`log_retention_days`、`artificial_analysis_api_key`、`providers`、`models` 等。

## agents

| 字段       | 类型 | 说明                         |
| ---------- | ---- | ---------------------------- |
| id         | TEXT | PK（8 个内置 Agent ID）      |
| name       | TEXT | 显示名                       |
| mode       | TEXT | `native` / `auto` / `manual` |
| model_id   | TEXT | 可空                         |
| api_key    | TEXT |                              |
| endpoint   | TEXT |                              |
| updated_at | TEXT | RFC3339                      |

内置 ID：`claude-code`、`codex`、`opencode`、`hermes`、`kilocode`、`openclaw`、`pi`、`reasonix`。

## routes

| 字段             | 类型    | 说明                                                    |
| ---------------- | ------- | ------------------------------------------------------- |
| id               | TEXT    | PK                                                      |
| name             | TEXT    |                                                         |
| agent_pattern    | TEXT    | 匹配 Agent 标识（glob）                                 |
| routing_strategy | TEXT    | auto/balanced/cheapest/intelligent/speed/agentic 或模型 |
| model_id         | TEXT    |                                                         |
| fallback_ids     | TEXT    | JSON 数组字符串，默认 `[]`                              |
| priority         | INTEGER | 默认 0                                                  |
| enabled          | INTEGER | 0/1，默认 1                                             |
| created_at       | TEXT    |                                                         |
| updated_at       | TEXT    |                                                         |

## request_logs

| 字段                                        | 类型    | 说明      |
| ------------------------------------------- | ------- | --------- |
| id                                          | TEXT    | PK        |
| timestamp                                   | TEXT    |           |
| agent / provider / model                    | TEXT    |           |
| input_tokens / output_tokens / total_tokens | INTEGER |           |
| latency_ms                                  | INTEGER |           |
| status                                      | INTEGER | HTTP 状态 |
| error                                       | TEXT    | 可空      |
| path                                        | TEXT    |           |
| stream                                      | INTEGER | 0/1       |
| cache_read_tokens                           | INTEGER |           |
| cache_creation_tokens                       | INTEGER |           |
| request_body / response_body                | TEXT    | 可空      |

索引：`timestamp`、`agent`、`provider`。保留天数由 `settings.log_retention_days`（默认 30）控制。

## usage_records

用量明细（provider / model / service_provider / agent、token、cost_usd、subscription 等）。

## subscription_quotas

按 `provider_id` + 周期记录 token 上限与已用量。

## catalog_providers / catalog_models / model_endpoints

models.dev 同步结果；`data` 列为 JSON。`model_endpoints` 存端点级定价与元数据。

## aa_benchmark_records

Artificial Analysis 基准记录（`slug` PK，`data` JSON，`synced_at`）。

## 辅助磁盘路径（非主配置）

| 路径              | 用途                  |
| ----------------- | --------------------- |
| `~/.cab/catalog/` | models.dev 等下载缓存 |
| `~/.cab/logos/`   | 提供商 logo 缓存      |
