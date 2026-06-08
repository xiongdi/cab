---
title: 功能验证
description: 端到端验证 CAB 软件规格功能
chapter: system-test
order: 2
---

## 管理面功能

对照 `requirements/functional-requirements.md`：

| 功能 | 验证步骤 | 期望 |
| --- | --- | --- |
| Dashboard | 打开 `/`，触发若干 Gateway 请求 | 图表数字更新 |
| Providers 列表 | `/providers` | 显示同步供应商、端点、Key |
| 订阅标记 | 切换 subscribed 保存 | settings 持久化，路由偏好订阅 |
| Models 目录 | `/models` | 三源数据、启用开关 |
| Routes | `/routes` | 内置策略说明、候选预览 |
| Agents | `/agents` 改 auto | 配置文件指向 CAB |
| Logs | `/logs` 筛选 | 分页、字段完整 |
| Settings | 改 gateway_port、同步目录 | 保存成功、catalog-status 更新 |

## 数据面功能

| 功能 | 验证 |
| --- | --- |
| OpenAI Chat | POST `/v1/chat/completions` 返回 choices |
| OpenAI Responses | POST `/v1/responses` |
| Anthropic | POST `/v1/messages` |
| Gemini | POST `/v1beta/models/{model}:generateContent` |
| Models 列表 | GET `/v1/models` 仅 enabled 模型 |

## 路由功能

| 策略 | 验证 |
| --- | --- |
| auto | 简单 prompt → 低成本模型；复杂 coding → 高 capability |
| balanced | 能力/成本比排序 |
| cheapest | 最低 effective_cost |
| intelligent | 按 coding_index 排序 |

## 订阅与 429

1. 配置 subscribed Key
2. 路由成本优先该供应商
3. 模拟 429 → UI 显示额度恢复时间；自动 fallback 按量 Key

## 界面结构

Sidebar 7 项与 `architecture-and-ui.md` 一致；中英文切换正常（`translations.ts`）。
