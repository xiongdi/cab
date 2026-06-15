---
title: 功能验证
description: 端到端验证 CAB 软件规格功能
chapter: system-test
order: 2
---

## 管理面功能

对照 `requirements/functional-requirements.md`：

| 功能           | 验证步骤                        | 期望                                          |
| -------------- | ------------------------------- | --------------------------------------------- |
| Dashboard      | 打开 `/`，触发若干 Gateway 请求 | 图表数字更新                                  |
| Providers 列表 | `/providers`                    | 显示 models.dev 同步的 LLM 提供商、端点与 Key |
| Provider Keys  | 配置多 Key、调整顺序保存        | settings 持久化，fallback 按配置顺序尝试      |
| Models 目录    | `/models`                       | 三源数据、启用开关                            |
| Routes         | `/routes`                       | 内置策略说明、候选预览                        |
| Agents         | `/agents` 改 auto               | 配置文件指向 CAB                              |
| Logs           | `/logs` 筛选                    | 分页、字段完整                                |
| Settings       | 改 gateway_port、同步目录       | 保存成功、catalog-status 更新                 |

## 数据面功能

| 功能             | 验证                                     |
| ---------------- | ---------------------------------------- |
| OpenAI Chat      | POST `/v1/chat/completions` 返回 choices |
| OpenAI Responses | POST `/v1/responses`                     |
| Anthropic        | POST `/v1/messages`                      |
| Models 列表      | GET `/v1/models` 仅 enabled 模型         |

## 路由功能

| 策略        | 验证                                                  |
| ----------- | ----------------------------------------------------- |
| auto        | 简单 prompt → 低成本模型；复杂 coding → 高 capability |
| balanced    | 主能力/有效成本比；∞ 时按 capability 降序 tie-break |
| cheapest    | effective_cost 最低                                   |
| intelligent | coding_index 降序                                     |
| speed       | output_speed 降序；无数据降级 cheapest                |

## 429 与 Key fallback

1. 为同一 Provider 配置多个 Key
2. 模拟 429 → `quota_reset_at` 持久化，该 Key 被跳过
3. `ordered_api_keys` 按配置顺序尝试下一个可用 Key

## 界面结构

Sidebar 7 项与 `architecture-and-ui.md` 一致；中英文切换正常（`translations.ts`）。
