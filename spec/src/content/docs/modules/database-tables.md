---
title: 数据库表设计
description: CAB 内存存储与 settings.json 结构
chapter: modules
order: 3
---

CAB **不使用关系型数据库**。本节将 `StoreData` 与 `settings.json` 映射为逻辑表，便于测试与追溯。

## 逻辑表：providers

| 字段           | 类型               | 约束         |
| -------------- | ------------------ | ------------ |
| id             | string             | PK           |
| name           | string             |              |
| endpoints      | ProviderEndpoint[] |              |
| api_keys       | ApiKeyConfig[]     |              |
| api_key        | string             | 派生首选 Key |
| enabled        | bool               |              |
| catalog_models | string[]           |              |

索引：`HashMap<id, Provider>`

## 逻辑表：models

| 字段                     | 类型   | 约束                                       |
| ------------------------ | ------ | ------------------------------------------ |
| id                       | string | PK（规范化 name）                          |
| name                     | string | canonical slug                             |
| provider_id              | string | FK → providers.id                          |
| protocol                 | string | openai-chat / anthropic / openai-responses |
| input_cost / output_cost | f64?   |                                            |
| coding_index 等          | f64    | AA 或启发式                                |
| enabled                  | bool   |                                            |

## 逻辑表：model_endpoints

| 字段                     | 类型   | 说明                                |
| ------------------------ | ------ | ----------------------------------- |
| key                      | string | PK，通常为 `provider_id:model_name` |
| provider_id              | string |                                     |
| model_id                 | string |                                     |
| input_cost / output_cost | f64?   | 端点级定价覆盖                      |

## 逻辑表：routes

| 字段               | 类型     | 说明                               |
| ------------------ | -------- | ---------------------------------- |
| id                 | string   | PK                                 |
| name               | string   |                                    |
| agent_pattern      | string   | 匹配 Agent 标识                    |
| routing_strategy   | string   | auto/balanced/cheapest/intelligent |
| primary_model_id   | string?  |                                    |
| fallback_model_ids | string[] |                                    |

## 逻辑表：agents

| 字段               | 类型    | 说明                   |
| ------------------ | ------- | ---------------------- |
| id                 | string  | PK，7 个内置           |
| mode               | string  | native / auto / manual |
| model_id           | string? | manual 模式            |
| api_key / endpoint | string  | 透传配置               |

## 逻辑表：request_logs

| 字段                         | 类型   | 说明    |
| ---------------------------- | ------ | ------- |
| id                           | uuid   |         |
| agent / provider / model     | string |         |
| input_tokens / output_tokens | i64    |         |
| latency_ms                   | i64    |         |
| status_code                  | i32    |         |
| created_at                   | string | RFC3339 |

存储：JSONL 文件 `~/.cab/logs/requests-YYYY-MM-DD.jsonl`；内存缓存最近 500 条；保留天数 `settings.log_retention_days`（默认 30）。

## 持久化文件：~/.cab/settings.json

```json
{
  "gateway_port": 3125,
  "log_retention_days": 30,
  "gateway_key": "cab-token-<uuid>",
  "auth_enabled": true,
  "artificial_analysis_api_key": null,
  "providers": { "<id>": { "enabled": true, "api_keys": [...] } },
  "models": { "<id>": { "enabled": true } }
}
```

## 持久化文件：~/.cab/state.json

```json
{
  "version": 1,
  "agents": { "<id>": { "mode": "native", "model_id": null, "...": "..." } },
  "routes": { "<id>": { "agent_pattern": "codex", "routing_strategy": "auto", "...": "..." } }
}
```

目录缓存：`~/.cab/catalog/`（models.dev、AA 快照）。
