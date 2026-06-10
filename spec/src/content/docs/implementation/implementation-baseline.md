---
title: 实现依据与顺序
description: CAB v0.2.0 编码顺序与模组设计追溯
chapter: implementation
order: 1
---

## v0.2.0 实现顺序

1. **文档**：requirements → architecture → system-design → modules → acceptance
2. **P0**：`state.json`、auth settings、auth middleware
3. **P1**：`cab-services` crate、`RouteCatalog` trait
4. **P2**：AgentIntegration registry、ProtocolAdapter
5. **P3**：JSONL logs、routing explain、OpenAPI
6. **收尾**：版本 0.2.0、CHANGELOG、CI openapi 校验

## 功能增量追溯

| 需求 ID                | 实现域                                        | 状态 |
| ---------------------- | --------------------------------------------- | ---- |
| REQ-CAB-001 统一网关   | `cab-gateway`, `cab-server`                   | 已有 |
| REQ-CAB-002 智能路由   | `cab-core/routing.rs`, `route_resolver.rs`    | 已有 |
| REQ-CAB-003 提供商管理 | `cab-services/catalog.rs`                     | P1   |
| REQ-CAB-004 订阅与 429 | `subscription_quota.rs`, `fallback.rs`        | 已有 |
| REQ-CAB-005 可观测性   | `log_store.rs`, `dashboard.rs`                | P3   |
| REQ-CAB-006 Agent 接入 | `cab-services/agents/`                        | P2   |
| REQ-CAB-007 配置持久化 | `cab-db/state.rs`                             | P0   |
| REQ-CAB-008 鉴权       | `cab-gateway/auth.rs`                         | P0   |
| REQ-CAB-009 JSONL 日志 | `cab-db/log_store.rs`                         | P3   |
| REQ-CAB-010 路由解释   | `route_explainer.rs`, `routing.rs`, Routes UI | P3   |

## 构建产物

- Rust：`target/debug/cab-server` 或 release 二进制
- 前端：`build/`（SvelteKit adapter-static）
- OpenAPI：`spec/src/content/docs/modules/openapi.yaml`
- Tauri：平台安装包（`src-tauri`）
