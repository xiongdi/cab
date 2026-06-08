---
title: 用户需要分析
description: CAB 目标用户与核心痛点，源自 README 与 agents 模块
chapter: requirements
order: 1
---

## 目标用户

| 用户群 | 工具 | 需要 |
| --- | --- | --- |
| 编程智能体用户 | Claude Code、Codex、OpenCode、Hermes 等 | 不改 SDK 即可接入多模型 |
| 成本敏感开发者 | 多供应商 Key 持有者 | 自动选性价比最高的模型 |
| 订阅用户 | Claude Pro/Max、ChatGPT Plus 等 | 优先消耗已预付额度 |
| 本地部署用户 | Ollama 用户 | 零 API 成本本地推理 |

## 核心用户故事（源自 README）

1. **统一网关接入**：Agent/SDK 指向 `http://127.0.0.1:{gateway_port}`，通过统一 `/v1/*`、`/v1beta/*` 路由转发。
2. **能力+成本路由**：按 Intelligence Index、Coding Index、Agentic Index 与 token 价格动态选模型（`routing.rs`）。
3. **目录自动同步**：models.dev 保持模型/定价/基准最新（`providers.rs::sync_models_dev_catalog`）。
4. **可视化管理**：Tauri 桌面端配置供应商、模型、路由、日志（`src/routes/*`）。

## 约束

- 本地运行，数据不出本机（除转发至上游 LLM）
- 设置存于 `~/.cab/settings.json`，不上传云端

## CAB 待填

- 目标用户访谈记录
- 竞品对比（LiteLLM、OpenRouter 本地代理等）
