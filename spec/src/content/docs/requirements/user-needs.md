---
title: 用户需要分析
description: CAB 目标用户与核心痛点，源自 README 与 agents 模块
chapter: requirements
order: 1
---

## 目标用户

| 用户群         | 工具                                    | 需要                    |
| -------------- | --------------------------------------- | ----------------------- |
| 编程智能体用户 | Claude Code、Codex、OpenCode、Hermes 等 | 不改 SDK 即可接入多模型 |
| 成本敏感开发者 | 多提供商 Key 持有者                     | 自动选性价比最高的模型  |
| 订阅用户       | Claude Pro/Max、ChatGPT Plus 等         | 优先消耗已预付额度      |
| 本地部署用户   | Ollama 用户                             | 零 API 成本本地推理     |

## 核心用户故事（源自 README）

1. **统一网关接入**：Agent/SDK 指向 `http://127.0.0.1:{gateway_port}`，通过统一 `/v1/*` 路由转发。
2. **能力+成本路由**：按 Intelligence Index、Coding Index、Agentic Index 与 token 价格动态选模型（`routing.rs`）。
3. **目录自动同步**：models.dev 保持模型/定价/基准最新（`providers.rs::sync_models_dev_catalog`）。
4. **可视化管理**：Tauri 桌面端配置提供商、模型、路由、日志（`src/routes/*`）。

## 约束

- 本地运行，数据不出本机（除转发至上游 LLM）
- 用户配置存于 `~/.cab/cab.db`（SQLite），不上传云端
- 默认仅监听 `127.0.0.1`（`cab.toml` 可配置）

## 非功能需求（NFR）

| ID      | 维度   | 要求                                                      |
| ------- | ------ | --------------------------------------------------------- |
| NFR-001 | 可用性 | 进程重启后 Agent 模式、Route 规则、用户 Key 覆盖必须保留  |
| NFR-002 | 安全   | Gateway 与管理 API 默认启用 Bearer 鉴权（`gateway_key`）  |
| NFR-003 | 可观测 | 请求日志写入 SQLite `request_logs`，保留天数由 `log_retention_days` 控制 |
| NFR-004 | 性能   | 单用户本地场景；路由决策 < 50ms（不含上游 LLM 延迟）      |
| NFR-005 | 兼容性 | OpenAI / Anthropic SDK 协议 shim；7 种预置 Agent 无感接入 |

## 竞品定位（简要）

| 产品              | CAB 差异点                                            |
| ----------------- | ----------------------------------------------------- |
| LiteLLM Proxy     | CAB 聚焦 Coding Agent（识别、配置改写、订阅额度感知） |
| OpenRouter        | CAB 本地运行，数据与 Key 不出本机                     |
| 各 Agent 原生配置 | CAB 统一网关 + 能力/成本路由，无需改 SDK              |
