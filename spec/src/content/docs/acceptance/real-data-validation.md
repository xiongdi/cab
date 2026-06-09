---
title: 真实数据验证
description: 使用实际 API Key 与 Agent 流量验收
chapter: acceptance
order: 3
---

## 原则

验收须使用**用户真实**提供商 Key 与日常 Agent 任务，而非仅 mock 数据。

## 验证项

### 真实 Key 转发

1. 在 Providers 配置生产 Key（订阅 + 按量各一更佳）
2. 通过 Gateway 发送真实 coding 请求
3. 核对上游账单或 Logs 中 provider/model/token

### 真实目录

1. Settings 同步 models.dev / AA
2. Models 页显示与官网一致的模型名与定价量级
3. 启用/禁用立即影响 `/v1/models` 列表

### 真实 Agent 流量

| Agent        | 验证         |
| ------------ | ------------ |
| Claude Code  | 终端多轮对话 |
| Codex / 其他 | 至少抽样一种 |

### 订阅与 429（真实）

若用户有订阅套餐：

1. 标记 subscribed → 观察是否优先消耗订阅路由
2. 若遇真实 429 → UI 显示 `quota_reset_at`，流量 fallback 按量 Key

## 数据隐私

- Logs 存于本地内存/进程，不上传云端
- 验收记录脱敏：不提交完整 Key 到 spec 仓库

## 通过标准

- 真实任务连续 30 分钟无 Blocker
- Token 统计与主观体验一致
- 订阅/fallback 行为符合用户预期
