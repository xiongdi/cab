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

## 0.2.0 功能变更摘要

### 订阅路由

- `ApiKeyConfig.subscribed`（Rust + TS）
- `effective_routing_cost` 订阅提供商近零边际成本
- `ordered_api_keys` 优先订阅 Key

### 429 Fallback

- `subscription_quota.rs` 解析 Retry-After
- `quota_reset_at` 持久化到 settings
- `execute_with_fallback` 多 Key × 模型重试
- Providers UI 订阅开关与额度标签

## 变更流程

1. 提出变更 → 更新 requirements 基线
2. 设计评审 → 更新 system-design / architecture / modules
3. 实现 → 更新 implementation
4. 测试 → 更新对应 validation 章
5. 本表追加一行

## 待办基线项

（无开放项时留空或写「无」）
