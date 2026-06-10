---
title: 接口与通信契约
description: Gateway、管理 API 与存储层契约
chapter: architecture
order: 3
---

## Gateway 对外契约

| 方法 | 路径                   | Handler                           | 协议               |
| ---- | ---------------------- | --------------------------------- | ------------------ |
| POST | `/v1/chat/completions` | `openai::handle_chat_completions` | OpenAI Chat        |
| POST | `/v1/responses`        | `openai::handle_responses`        | OpenAI Responses   |
| POST | `/v1/messages`         | `anthropic::handle_messages`      | Anthropic Messages |
| GET  | `/v1/models`           | `openai::handle_list_models`      | OpenAI Models      |

**认证**：见 [`security-model.md`](security-model.md)。`auth_enabled == true` 时全部 `/v1/*` 与 `/api/*` 须 `Authorization: Bearer {gateway_key}`。

**路由解析**：`cab_services::route_resolver::resolve_route(catalog, agent, model, body)` → `ResolvedRoute`。

## 管理 API 契约（`/api`）

完整路由见 `cab-api/src/lib.rs`。新增：

| 方法 | 路径                   | 说明         |
| ---- | ---------------------- | ------------ |
| POST | `/api/routing/explain` | 路由决策解释 |

统一响应：

- 成功：`200` + JSON 实体
- 未授权：`401`
- 未找到：`CabError::NotFound` → 4xx
- 数据库错误：`CabError::Database`

CORS：`CorsLayer` 允许任意 Origin（本地管理 UI）；鉴权由 Bearer 保障。

## state.json 契约

路径：`~/.cab/state.json`

```json
{
  "version": 1,
  "agents": { "<id>": { "id", "name", "mode", "model_id", "api_key", "endpoint", "updated_at" } },
  "routes": { "<id>": { "id", "name", "agent_pattern", "routing_strategy", "model_id", "fallback_ids", "priority", "enabled", "created_at", "updated_at" } }
}
```

- Agent/route 变更后原子写盘
- `init_store` 启动时加载并合并到 `StoreData`

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

```rust
pub struct StoreData {
    providers: HashMap<String, Provider>,
    models: HashMap<String, Model>,
    routes: HashMap<String, Route>,
    agents: HashMap<String, Agent>,
    request_logs: Vec<RequestLog>,
    settings: Settings,
    model_endpoints: HashMap<String, ModelEndpoint>,
}
```

- 读：`inner.read()`
- 写：`inner.write()` + `settings::save_to_disk` 或 `state::save_from_store`
- 日志：`log_store::append` → JSONL

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
