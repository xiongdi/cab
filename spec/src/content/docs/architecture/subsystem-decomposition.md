---
title: 子系统划分
description: CAB 上下文、容器与子系统边界
chapter: architecture
order: 1
---

## 上下文与容器（架构视图）

```
Coding Agent ──HTTP──► CAB（网关 + 管理 UI）──HTTPS──► 上游 LLM
                              │
                              ├── models.dev 目录
                              └── Artificial Analysis 基准
```

| 容器               | 技术                   | 说明                           |
| ------------------ | ---------------------- | ------------------------------ |
| cab-server / Tauri | Axum                   | HTTP gateway_port（默认 3125） |
| 管理前端           | SvelteKit `build/`     | 由 `ServeDir` 托管             |
| 持久化             | `~/.cab/settings.json` | 用户 Key、订阅标记             |

## Rust Workspace 子系统

| Crate         | 职责                                   | 对外接口                           |
| ------------- | -------------------------------------- | ---------------------------------- |
| `cab-core`    | 领域类型、路由算法、目录解析、错误类型 | 库 API，无 HTTP                    |
| `cab-db`      | 内存存储、settings 落盘、CRUD          | `InMemoryStore` + 各 `*_` 模块函数 |
| `cab-api`     | 管理 REST API                          | `/api/*` Axum 路由                 |
| `cab-gateway` | LLM 代理与路由执行                     | `/v1/*`                            |
| `cab-server`  | 进程入口、静态资源                     | `main()`                           |
| `src-tauri`   | 桌面壳，复用上述路由                   | Tauri 命令 + 内嵌 Axum             |

## 前端子系统（`src/`）

| 目录                  | 职责                                                                         |
| --------------------- | ---------------------------------------------------------------------------- |
| `routes/`             | 7 个管理页面（Dashboard、Providers、Models、Routes、Agents、Logs、Settings） |
| `lib/api.ts`          | 管理 API 客户端                                                              |
| `lib/types.ts`        | 与 `cab-core::types` 对齐的 TS 类型                                          |
| `lib/translations.ts` | 中英文 i18n                                                                  |

## 配置子系统（`config/`）

| 文件                               | 用途                           |
| ---------------------------------- | ------------------------------ |
| `provider-endpoints.defaults.json` | LLM 提供商默认上游端点模板     |
| `aa-model-map.json`                | Artificial Analysis 模型名映射 |

## 集成单元

最小可集成单元为 **Axum Router 合并体**：

```rust
gateway.merge(api).fallback_service(serve_dir)
```

`cab-server` 与 Tauri 均以此模式组装，保证集成测试可针对单一进程验证。
