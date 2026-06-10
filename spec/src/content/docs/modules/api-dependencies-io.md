---
title: 接口、依赖与输入输出
description: 各模组 I/O 与外部依赖（v0.2.0）
chapter: modules
order: 4
---

## cab-services 关键 I/O

### `route_resolver::resolve_route`

| 输入                                          | 输出                          |
| --------------------------------------------- | ----------------------------- |
| `RouteCatalog`, agent, requested_model?, body | `ResolvedRoute` 或 `CabError` |

### `route_explainer::explain`

| 输入                  | 输出                 |
| --------------------- | -------------------- |
| `RouteExplainRequest` | `RouteExplainResult` |

### `agent_config::update`

| 输入                   | 输出                          |
| ---------------------- | ----------------------------- |
| store, id, UpdateAgent | Agent + AgentIntegration 写盘 |

### `catalog::sync_models_dev_catalog`

| 输入  | 输出         |
| ----- | ------------ |
| store | 同步模型计数 |

## cab-gateway 关键 I/O

### `execute_with_fallback`

| 输入                                            | 输出                     |
| ----------------------------------------------- | ------------------------ |
| `Client`, `ResolvedModel`, `ProxyRequest`, pool | `Response` 或 `CabError` |

### `auth_middleware`

| 输入               | 输出                 |
| ------------------ | -------------------- |
| Request + settings | 401 或 next.run(req) |

## cab-api 模组 I/O 摘要

| Handler           | 委托至                          |
| ----------------- | ------------------------------- |
| `update_agent`    | `cab_services::agent_config`    |
| `sync_catalog`    | `cab_services::catalog`         |
| `explain_routing` | `cab_services::route_explainer` |

## cab-db 依赖

- **仅依赖** `cab-core`
- 磁盘 IO：`settings.rs`、`state.rs`、`log_store.rs`

## OpenAPI 生成流程

1. `cab-api` 使用 `utoipa` 注解
2. `scripts/generate-openapi.sh` 导出至 `spec/src/content/docs/modules/openapi.yaml`
3. `scripts/generate-api-types.mjs` 生成 `src/lib/api-types.ts`

## 前端 api.ts 方法映射

| 函数             | HTTP                        |
| ---------------- | --------------------------- |
| `fetchProviders` | GET `/api/providers`        |
| `updateAgent`    | PUT `/api/agents/{id}`      |
| `explainRouting` | POST `/api/routing/explain` |
| `updateSettings` | PUT `/api/settings`         |

## 错误消息约定

`CabError` 变体：`NotFound`, `Database`, `ProviderError`, `InvalidRequest`, `Internal` 等，由 Axum `IntoResponse` 转为 HTTP 状态码与 JSON `{ "error": "..." }`。未授权返回 `401`。
