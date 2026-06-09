---
title: 术语与缩写
description: CAB 项目专用术语，源自源码命名
chapter: preface
order: 3
---

## 核心术语

| 术语            | 源码定义             | 说明                                                           |
| --------------- | -------------------- | -------------------------------------------------------------- |
| Gateway         | `cab-gateway`        | 转发 OpenAI/Anthropic API 的本地 HTTP 网关                     |
| Management API  | `cab-api`            | 挂载于 `/api` 的配置、目录、日志接口                           |
| InMemoryStore   | `cab-db/src/lib.rs`  | `RwLock<StoreData>` 内存数据库，设置持久化到 JSON              |
| RequestProfile  | `routing.rs`         | 请求分类结果：`task` + `complexity` + `estimated_input_tokens` |
| RoutingStrategy | `routing.rs`         | `auto` / `balanced` / `cheapest` / `intelligent`               |
| ApiKeyConfig    | `types.rs`           | `key`, `enabled`, `subscribed`, `quota_reset_at`               |
| Provider        | `types.rs`           | LLM API 提供商（如 OpenAI、Anthropic），目录来自 models.dev    |
| ModelEndpoint   | `cab-db/endpoint.rs` | models.dev 同步的模型-提供商定价矩阵行                         |
| ResolvedRoute   | `gateway/router.rs`  | 解析后的主模型 + 最多 2 个 fallback                            |

## 路由策略语义（源码）

| 策略          | `rank_models` 行为                        |
| ------------- | ----------------------------------------- |
| `auto`        | 任务加权能力分 + 复杂度门槛 + 能力/成本   |
| `balanced`    | 主能力指数 / 有效成本（input×3 + output） |
| `cheapest`    | 负有效成本排序                            |
| `intelligent` | 纯 `coding_index` 排序，忽略成本          |

## 缩写

| 缩写        | 含义                           |
| ----------- | ------------------------------ |
| AA          | Artificial Analysis 基准数据源 |
| URD         | User Requirements Document     |
| UAT         | User Acceptance Test           |
| UTP/ITP/STP | 单元/集成/系统测试计划         |

## Agent 内置 ID

`cab-db/src/lib.rs` 预置：`claude-code`、`codex`、`opencode`、`hermes`、`kilocode`、`openclaw`、`pi`。
