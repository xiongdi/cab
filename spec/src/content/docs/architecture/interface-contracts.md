---
title: 接口与通信契约
description: Gateway、管理 API 与存储层契约
chapter: architecture
order: 3
---

## Gateway 对外契约

| 方法 | 路径                   | 协议               |
| ---- | ---------------------- | ------------------ |
| POST | `/v1/chat/completions` | OpenAI Chat        |
| POST | `/v1/responses`        | OpenAI Responses   |
| GET  | `/v1/responses`        | Responses WebSocket |
| POST | `/v1/messages`         | Anthropic Messages |
| GET  | `/v1/models`           | OpenAI Models      |

**认证**：见 [`security-model.md`](security-model.md)。`auth_enabled == true` 时 `/v1/*` 须 `Authorization: Bearer {gateway_key}`（亦接受 `x-api-key`）。`/api/*` 同理，本机仪表盘 Origin/Referer 可绕过。

**路由解析**：`cab_services::route_resolver::resolve_route(...)` → `ResolvedRoute`。

## 管理 API 契约（`/api`）

完整路由见 `cab-api/src/lib.rs`（含 providers、models、routes、agents、logs、usage、settings、routing/explain、routing/strategy-board、diagnostics、dashboard、update 等）。

统一响应：

- 成功：`200` + JSON 实体
- 未授权：`401`
- 未找到：`CabError::NotFound` → 4xx
- 数据库错误：`CabError::Database`

CORS：`CorsLayer` 允许任意 Origin（本地管理 UI）；鉴权由 Bearer 保障。

## SQLite 存储契约

路径：`~/.cab/cab.db`。表结构见 [`database-tables.md`](../modules/database-tables.md) 与 `crates/cab-db/src/sqlite.rs`。

- `settings`（`id=1` JSON）：端口、gateway_key、auth、providers/models 覆盖
- `agents` / `routes`：Agent 模式与路由规则
- `request_logs` / `usage_records`：观测数据
- `catalog_*` / `model_endpoints` / `aa_benchmark_records`：目录与基准

已废弃文件契约：`settings.json` / `state.json` / JSONL 日志。

## RouteCatalog trait

```rust
#[async_trait]
pub trait RouteCatalog: Send + Sync {
    async fn agent(&self, id: &str) -> Result<Option<Agent>, CabError>;
    async fn routes_for_agent(&self, agent: &str) -> Result<Vec<Route>, CabError>;
    async fn enabled_models(&self) -> Result<Vec<Model>, CabError>;
    async fn model_by_id(&self, id: &str) -> Result<Option<Model>, CabError>;
    async fn provider_by_id(&self, id: &str) -> Result<Option<Provider>, CabError>;
}
```

`InMemoryStore` 实现该 trait；`cab-services::route_resolver` 仅依赖 trait。

## RouteExplain 契约

**请求** `POST /api/routing/explain`：

```json
{
  "agent": "codex",
  "model": "auto",
  "body": { "messages": [{ "role": "user", "content": "..." }] }
}
```

**响应**：

```json
{
  "resolved": { "model_id": "...", "provider_id": "...", "strategy": "auto" },
  "decision_steps": [{ "step": "agent_auto_route", "matched": true, "detail": "..." }],
  "ranked_candidates": [{ "model_id": "...", "capability": 0.85, "value": 12.3 }]
}
```

## cab-db 内部契约

内存侧 `InMemoryStore` / `StoreData` 持有 providers、models、routes、agents、request_logs、settings、model_endpoints 等；启动时从 SQLite 水合。

- 读：`inner.read()`
- 写：`inner.write()` + `settings` / `state` / catalog / logs 模块写入 SQLite
- 日志：写入 `request_logs` 表（非 JSONL）

## 错误与 Fallback 契约

`proxy.rs` 将上游 429 转为：

```rust
CabError::ProviderError { retry_after: Option<u64> }
```

`fallback.rs::execute_with_fallback` 顺序：

1. 主模型 × `ordered_api_keys` × 端点候选
2. `fallback_models` 列表
3. 全部失败 → 向上返回最后错误

## 前端 ↔ API

`api.ts` 基址：`http://127.0.0.1:{gateway_port}/api`（Tauri 通过 `get_gateway_port` 获取端口）。
