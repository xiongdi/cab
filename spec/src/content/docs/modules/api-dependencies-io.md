---
title: 接口、依赖与输入输出
description: 各模组 I/O 与外部依赖
chapter: modules
order: 4
---

## cab-gateway 关键 I/O

### `resolve_route`

| 输入                                           | 输出                          |
| ---------------------------------------------- | ----------------------------- |
| `InMemoryStore`, agent, requested_model?, body | `ResolvedRoute` 或 `CabError` |

`ResolvedRoute` 含：`model`, `provider_id`, `api_keys`, `endpoint_candidates`, `fallback_models`。

### `execute_with_fallback`

| 输入                                            | 输出                     |
| ----------------------------------------------- | ------------------------ |
| `Client`, `ResolvedModel`, `ProxyRequest`, pool | `Response` 或 `CabError` |

### `proxy_request`

| 输入                                 | 输出                                |
| ------------------------------------ | ----------------------------------- |
| endpoint URL, api_key, body, headers | 上游 HTTP 响应；异步写 `RequestLog` |

## cab-api 模组 I/O 摘要

| Handler                     | 输入            | 输出                       |
| --------------------------- | --------------- | -------------------------- |
| `list_providers`            | —               | `Vec<Provider>`            |
| `sync_models_dev_providers` | —               | 同步计数                   |
| `list_model_catalog`        | —               | models_dev + AA + settings |
| `update_agent`              | `UpdateAgent`   | `Agent` + 副作用写配置文件 |
| `query_logs`                | 分页/筛选 query | `RequestLog` 页            |
| `get_stats`                 | —               | Dashboard 聚合             |
| `sync_catalog`              | —               | 触发双源同步               |

## cab-db 依赖

- **仅依赖** `cab-core::types`
- **无** reqwest、axum
- 磁盘 IO 仅限 `settings.rs`

## cab-core 外部依赖

| 依赖               | 用途   |
| ------------------ | ------ |
| serde / serde_json | 序列化 |
| chrono             | 时间戳 |
| （无 HTTP 客户端） | 纯计算 |

## 前端 api.ts 方法映射

| 函数                  | HTTP                       |
| --------------------- | -------------------------- |
| `fetchProviders`      | GET `/api/providers`       |
| `updateProvider`      | PUT `/api/providers/{id}`  |
| `fetchModelCatalog`   | GET `/api/models/catalog`  |
| `updateAgent`         | PUT `/api/agents/{id}`     |
| `fetchDashboardStats` | GET `/api/dashboard/stats` |
| `updateSettings`      | PUT `/api/settings`        |

## 错误消息约定

`CabError` 变体：`NotFound`, `Database`, `ProviderError`, `BadRequest`, `Internal` 等，由 Axum `IntoResponse` 转为 HTTP 状态码与 JSON `{ "error": "..." }`。
