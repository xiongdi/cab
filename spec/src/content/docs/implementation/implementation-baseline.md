---
title: 实现依据与顺序
description: CAB 编码顺序与模组设计追溯
chapter: implementation
order: 1
---

## 实现依据

| 优先级 | 文档 | 代码落点 |
| --- | --- | --- |
| 1 | 模组设计 `logic-pseudocode.md` | `routing.rs`, `fallback.rs` |
| 2 | 接口契约 `interface-contracts.md` | `lib.rs`, `server.rs` |
| 3 | 数据字典 `entity-data-dictionary.md` | `types.rs`, `StoreData` |
| 4 | 系统规格 `software-specification.md` | `main.rs` 组装 |

## 推荐实现顺序

1. **cab-core**：类型与路由算法（无 IO，易测）
2. **cab-db**：Store + settings 持久化
3. **cab-gateway**：router → proxy → protocol → 协议 handler
4. **cab-api**：providers/models 同步 → 其余 CRUD → agents
5. **cab-server**：HTTP 启动、静态资源合并
6. **前端**：按 Dashboard → Providers → Models → Agents → 其余页面
7. **src-tauri**：复用 server 启动逻辑

## 功能增量追溯

| 需求 ID | 实现提交域 |
| --- | --- |
| REQ-CAB-001 统一网关 | `cab-gateway`, `cab-server` |
| REQ-CAB-002 智能路由 | `cab-core/routing.rs`, `router.rs` |
| REQ-CAB-003 供应商管理 | `cab-api/providers.rs`, `providers/+page.svelte` |
| REQ-CAB-004 Agent 接入 | `agents.rs` |
| REQ-CAB-005 可观测性 | `log.rs`, `dashboard.rs`, `logs/+page.svelte` |
| REQ-CAB-006 订阅与 429 | `subscription_quota.rs`, `types.rs`, `fallback.rs` |

## 构建产物

- Rust：`target/debug/cab-server` 或 release 二进制
- 前端：`build/`（SvelteKit adapter-static）
- Tauri：平台安装包（`src-tauri`）
