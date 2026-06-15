---
title: 变更记录
description: CAB spec 与产品基线变更历史
chapter: appendix
order: 2
---

## 记录格式

| 版本       | 日期    | 变更范围                | 影响分析                        |
| ---------- | ------- | ----------------------- | ------------------------------- |
| 0.1.0-spec | 2026-06 | 初始 spec 站 55 篇      | 建立 V 模型文档体系             |
| 0.2.0-spec | 2026-06 | 全量按源码手写          | 替换脚本占位内容                |
| 0.2.0-prod | 2026-06 | 订阅 Key + 429 fallback | REQ-CAB-006；types、routing、UI |
| 0.2.0-arch | 2026-06 | P0–P3 架构演进          | REQ-CAB-007~010；cab-services   |
| 0.2.1-spec | 2026-06 | 移除订阅路由文档漂移    | 路由成本改 models.dev 端点定价  |

## 0.2.0 架构演进摘要（v0.2.0-arch）

### 持久化与安全（P0）

- `~/.cab/state.json` 持久化 agents、routes
- `auth_enabled` + Gateway/API Bearer 鉴权
- 首次安装随机 `gateway_key`

### 应用服务层（P1）

- 新建 `cab-services` crate
- `RouteCatalog` trait；薄化 cab-api / cab-gateway

### 插件化（P2）

- `AgentIntegration` registry（7 Agent 独立模块）
- `ProtocolAdapter`（openai-chat / responses / anthropic）

### 可观测与 API（P3）

- JSONL 日志 + retention
- `POST /api/routing/explain` + Routes UI
- OpenAPI + 前端类型生成

## 0.2.0 功能变更摘要

### 路由成本与 Key 选择（当前）

- 路由成本：`effective_token_cost_for_model`，基于 models.dev 每提供商端点定价（`input_cost` / `output_cost` / `cache_read`）
- `ApiKeyConfig`：`key`、`enabled`、`quota_reset_at`
- `ordered_api_keys`：按 `api_keys` 配置顺序，跳过 rate-limited Key

### 429 Fallback

- `subscription_quota.rs` 解析 Retry-After
- `quota_reset_at` 持久化到 settings
- `execute_with_fallback` 多 Key × 模型重试
- Providers UI 额度恢复标签

## 变更流程

1. 提出变更 → 更新 requirements 基线
2. 设计评审 → 更新 system-design / architecture / modules
3. 实现 → 更新 implementation
4. 测试 → 更新对应 validation 章
5. 本表追加一行

## 待办基线项

（无开放项时留空或写「无」）
