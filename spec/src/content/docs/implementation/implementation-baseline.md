---
title: 实现依据与顺序
description: CAB 编码顺序与模组设计追溯（含历史 v0.2 与当前 SQLite）
chapter: implementation
order: 1
---

## 历史：v0.2.0 实现顺序

1. **文档**：requirements → architecture → system-design → modules → acceptance
2. **P0**：agents/routes 持久化、auth settings、auth middleware
3. **P1**：`cab-services` crate、`RouteCatalog` trait
4. **P2**：AgentIntegration registry、ProtocolAdapter
5. **P3**：请求日志、routing explain、OpenAPI
6. **收尾**：版本 0.2.0、CHANGELOG、CI openapi 校验

> 当时持久化为 `settings.json` / `state.json` / JSONL；后续版本已迁移至 SQLite。

## 当前实现（以代码为准）

| 需求 ID                | 实现域                                              | 状态 |
| ---------------------- | --------------------------------------------------- | ---- |
| REQ-CAB-001 统一网关   | `cab-gateway`, `cab-srv`                            | 已有 |
| REQ-CAB-002 智能路由   | `cab-core/routing.rs`, `route_resolver.rs`          | 已有 |
| REQ-CAB-003 提供商管理 | `cab-services/catalog.rs`                           | 已有 |
| REQ-CAB-004 订阅与 429 | `subscription_quota.rs`, `fallback.rs`              | 已有 |
| REQ-CAB-005 可观测性   | `cab-db/log.rs`, `dashboard.rs`, usage 表           | 已有 |
| REQ-CAB-006 Agent 接入 | `cab-services/agents/`（含 reasonix，共 8 个）      | 已有 |
| REQ-CAB-007 配置持久化 | `cab-db/sqlite.rs` + `settings`/`state`/`agents`…   | 已有 |
| REQ-CAB-008 鉴权       | `cab-gateway` / `cab-db/auth.rs`                    | 已有 |
| REQ-CAB-009 日志持久化 | SQLite `request_logs`                               | 已有 |
| REQ-CAB-010 路由解释   | `route_explainer.rs` + `strategy-board`             | 已有 |

## 构建产物

- Rust：`target/debug/cab-srv` 或 release 二进制
- 前端：`build/`（SvelteKit adapter-static）
- OpenAPI：`spec/src/content/docs/modules/openapi.yaml`
- Tauri：平台安装包（`src-tauri`）
- CLI：`cab-cli`（`crates/cab`）
