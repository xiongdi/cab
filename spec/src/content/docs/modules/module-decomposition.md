---
title: 模组拆解
description: CAB 可编码最小交付单元（v0.2.0）
chapter: modules
order: 1
---

## cab-core 模组

| 模组               | 文件                    | 职责                                                  |
| ------------------ | ----------------------- | ----------------------------------------------------- |
| types              | `types.rs`              | Provider、Model、Agent、Route、Settings、ApiKeyConfig |
| routing            | `routing.rs`            | RequestProfile、rank_models、策略解析                 |
| subscription_quota | `subscription_quota.rs` | Retry-After 解析、quota_reset_at 判断                 |
| benchmark_catalog  | `benchmark_catalog.rs`  | AA 基准拉取与合并                                     |
| model_scores       | `model_scores.rs`       | 启发式能力分数                                        |
| provider_defaults  | `provider_defaults.rs`  | 默认端点注入                                          |
| config             | `config.rs`             | `cab.toml` 加载                                       |
| error              | `error.rs`              | `CabError` 枚举                                       |

## cab-db 模组

| 模组      | 文件           | 职责                            |
| --------- | -------------- | ------------------------------- |
| store     | `lib.rs`       | `InMemoryStore`、`RouteCatalog` |
| state     | `state.rs`     | `state.json` 读写               |
| settings  | `settings.rs`  | `settings.json` 读写            |
| log_store | `log_store.rs` | JSONL 追加、retention           |
| provider  | `provider.rs`  | LLM 提供商 CRUD                 |
| model     | `model.rs`     | 模型 CRUD                       |
| endpoint  | `endpoint.rs`  | ModelEndpoint                   |
| route     | `route.rs`     | 路由规则 CRUD + state 持久化    |
| agent     | `agent.rs`     | Agent 更新 + state 持久化       |
| log       | `log.rs`       | RequestLog 查询（JSONL + 缓存） |
| dashboard | `dashboard.rs` | 统计聚合                        |

## cab-services 模组

| 模组            | 文件                 | 职责                      |
| --------------- | -------------------- | ------------------------- |
| catalog         | `catalog.rs`         | models.dev 同步           |
| benchmarks      | `benchmarks.rs`      | AA 目录同步               |
| agent_config    | `agent_config.rs`    | Agent 更新 + 配置文件改写 |
| agents/\*       | `agents/*.rs`        | AgentIntegration 插件     |
| route_resolver  | `route_resolver.rs`  | resolve_route 编排        |
| route_explainer | `route_explainer.rs` | 路由决策解释              |

## cab-gateway 模组

| 模组        | 文件            | 职责                             |
| ----------- | --------------- | -------------------------------- |
| server      | `server.rs`     | 路由表注册                       |
| auth        | `auth.rs`       | Bearer 鉴权中间件                |
| adapters/\* | `adapters/*.rs` | ProtocolAdapter 实现             |
| fallback    | `fallback.rs`   | 多 Key × 多端点 × 多模型重试     |
| proxy       | `proxy.rs`      | HTTP 上游请求、日志、429 处理    |
| protocol    | `protocol.rs`   | 跨协议 body 转换                 |
| openai      | `openai.rs`     | Chat/Responses/Models 薄 handler |
| anthropic   | `anthropic.rs`  | Messages 薄 handler              |
| agent_id    | `agent_id.rs`   | Agent 识别                       |
| state       | `state.rs`      | `GatewayState`                   |

## cab-api 模组

| 模组      | 文件           | 主要端点                    |
| --------- | -------------- | --------------------------- |
| providers | `providers.rs` | 薄 wrapper → catalog        |
| models    | `models.rs`    | 三源 catalog                |
| routes    | `routes.rs`    | 策略路由配置                |
| agents    | `agents.rs`    | 薄 wrapper → agent_config   |
| routing   | `routing.rs`   | POST `/api/routing/explain` |
| logs      | `logs.rs`      | 分页查询                    |
| dashboard | `dashboard.rs` | stats                       |
| settings  | `settings.rs`  | 网关端口、同步              |

## 前端页面模组

每页对应一个 Svelte route，通过 `api.ts` 调用单一资源域 API；`lib/api-types.ts` 由 OpenAPI 生成。
