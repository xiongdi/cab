---
title: 文档体系与追溯
description: CAB spec 章节与源码路径的映射规则
chapter: preface
order: 2
---

> 本站 **9 个正文章节** 对应维基百科 V 模型阶段；各章小节数量不等，见 [V 模型概述](/docs/preface/v-model-overview)。

## 追溯矩阵

| 需求/行为               | 设计文档章          | 源码位置                            | 测试验证                     |
| ----------------------- | ------------------- | ----------------------------------- | ---------------------------- |
| 成本感知路由            | 系统设计 / 模组设计 | `cab-core/src/routing.rs`           | `routing::tests::*`          |
| models.dev 端点定价路由 | 系统设计 / 模组设计 | `cab-core/src/routing.rs`           | `effective_token_cost_*`     |
| 429 额度恢复 fallback   | 需求分析 / 架构设计 | `fallback.rs`, `provider.rs`        | 手工/集成                    |
| models.dev 目录同步     | 系统设计            | `cab-api/src/providers.rs`          | `resolve_served_model_tests` |
| 智能体透明代理          | 架构设计            | `cab-gateway/src/server.rs`         | gateway router tests         |
| LLM 提供商 Key 管理 UI  | 需求分析            | `src/routes/providers/+page.svelte` | `npm run check`              |
| Agent 配置写入          | 系统设计            | `cab-api/src/agents.rs`             | 手工 UAT                     |

## 编号约定

- 功能需求：`REQ-CAB-###`（对应 GitHub Issue）
- 路由策略：`STR-auto`、`STR-balanced`、`STR-cheapest`、`STR-intelligent`
- LLM 提供商：`provider-{id}`（来自 models.dev，如 `anthropic`）
- 模型：`{provider}/{model}`（canonical slug）
- 测试用例：`UT-###`、`IT-###`、`ST-###`、`UAT-###`

## 持久化与配置追溯

| 文件            | 路径                                             | 写入方                              |
| --------------- | ------------------------------------------------ | ----------------------------------- |
| 运行时数据库    | `~/.cab/cab.db`                                  | `cab-db`（settings/agents/routes/logs/catalog） |
| models.dev 缓存 | `~/.cab/catalog/models.dev/catalog.json`         | catalog sync                        |
| AA 模型映射     | `config/aa-model-map.json` / catalog 缓存        | `cab-core` / sync                   |
| 内置端点默认    | `config/provider-endpoints.defaults.json`        | 随仓库发布                          |
| 系统引导        | `cab.toml`                                       | 用户放置；host + 端口种子           |

## 检查清单

- [ ] 每个 spec 小节引用至少一处真实源码路径
- [ ] API 变更同步更新 `src/lib/types.ts` 与 spec
- [ ] 配置项变更同步更新 `settings.rs` 默认值说明
