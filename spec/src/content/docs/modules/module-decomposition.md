---
title: 模组拆解
description: CAB 可编码最小交付单元
chapter: modules
order: 1
---

## cab-core 模组

| 模组 | 文件 | 职责 |
| --- | --- | --- |
| types | `types.rs` | Provider、Model、Agent、Route、Settings、ApiKeyConfig |
| routing | `routing.rs` | RequestProfile、rank_models、策略解析 |
| subscription_quota | `subscription_quota.rs` | Retry-After 解析、quota_reset_at 判断 |
| benchmark_catalog | `benchmark_catalog.rs` | AA 基准拉取与合并 |
| model_scores | `model_scores.rs` | 启发式能力分数 |
| provider_defaults | `provider_defaults.rs` | 默认端点注入 |
| config | `config.rs` | `cab.toml` 加载 |
| error | `error.rs` | `CabError` 枚举 |

## cab-db 模组

| 模组 | 文件 | 职责 |
| --- | --- | --- |
| store | `lib.rs` | `InMemoryStore`、`init_store`、7 内置 Agent |
| provider | `provider.rs` | 供应商 CRUD |
| model | `model.rs` | 模型 CRUD |
| endpoint | `endpoint.rs` | ModelEndpoint |
| route | `route.rs` | 路由规则 CRUD |
| agent | `agent.rs` | Agent 更新 |
| log | `log.rs` | RequestLog 追加与查询 |
| settings | `settings.rs` | 读写 `~/.cab/settings.json` |
| dashboard | `dashboard.rs` | 统计聚合 |

## cab-gateway 模组

| 模组 | 文件 | 职责 |
| --- | --- | --- |
| server | `server.rs` | 路由表注册 |
| router | `router.rs` | `resolve_route`、`ResolvedRoute` |
| fallback | `fallback.rs` | 多 Key × 多端点 × 多模型重试 |
| proxy | `proxy.rs` | HTTP 上游请求、日志、429 处理 |
| protocol | `protocol.rs` | 跨协议 body 转换 |
| openai | `openai.rs` | Chat/Responses/Models |
| anthropic | `anthropic.rs` | Messages |
| gemini | `gemini.rs` | generateContent |
| state | `state.rs` | `GatewayState` |

## cab-api 模组

| 模组 | 文件 | 主要端点 |
| --- | --- | --- |
| providers | `providers.rs` | 目录同步、余额查询 |
| models | `models.rs` | 三源 catalog |
| routes | `routes.rs` | 策略路由配置 |
| agents | `agents.rs` | Agent 模式、配置写入 |
| logs | `logs.rs` | 分页查询 |
| dashboard | `dashboard.rs` | stats |
| settings | `settings.rs` | 网关端口、同步 |
| benchmarks | `benchmarks.rs` | AA API 拉取 |

## 前端页面模组

每页对应一个 Svelte route，通过 `api.ts` 调用单一资源域 API。
