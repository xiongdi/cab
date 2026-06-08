---
title: 实体图与数据字典
description: CAB 核心实体，源自 types.rs 与 StoreData
chapter: system-design
order: 4
---

## 实体关系

```
Settings ──< ProviderUserSettings
Provider ──< ProviderEndpoint
Provider ──< ApiKeyConfig
Provider ──1─N── Model (provider_id)
Model ──1─N── ModelEndpoint (model_id / canonical_slug)
Route ──N──1── Model (primary_model_id)
Route ──N──M── Model (fallback_model_ids)
Agent ──0..1── Model|Route (model_id 依 mode)
RequestLog ──引用── Provider, Model, Agent
```

## Provider（`types.rs`）

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| id | String | models.dev 供应商标识 |
| name | String | 显示名 |
| endpoints | Vec\<ProviderEndpoint\> | 多协议端点 |
| api_keys | Vec\<ApiKeyConfig\> | 多 Key |
| api_key | String | 当前首选 Key（派生） |
| enabled | bool | 是否参与路由 |
| catalog_models | Vec\<String\> | 目录模型名列表 |

## ApiKeyConfig

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| key | String | 密钥明文（本地存储） |
| enabled | bool | 是否可用 |
| subscribed | bool | 订阅标记，影响路由成本 |
| quota_reset_at | Option\<String\> | RFC3339，429 后恢复时间 |

## Model

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| id | String | DB 主键（name 规范化） |
| name | String | canonical slug |
| provider_id | String | 归属供应商 |
| protocol | String | `openai-chat` / `anthropic` / `gemini` |
| input_cost / output_cost | Option\<f64\> | models.dev 定价 |
| coding_index 等 | f64 | AA 或启发式分数 |
| enabled | bool | 用户开关 |

## RequestLog

| 字段 | 说明 |
| --- | --- |
| agent | 客户端标识 |
| provider / model | 实际转发目标 |
| input_tokens / output_tokens | 用量 |
| latency_ms | 耗时 |
| status_code | HTTP 状态 |

## StoreData（`cab-db/lib.rs`）

`HashMap` 索引：`providers`、`models`、`routes`、`agents`、`model_endpoints`；`request_logs` 为 `Vec`。
