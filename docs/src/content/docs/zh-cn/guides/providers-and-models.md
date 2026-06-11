---
title: 提供商与模型
description: 在 CAB 中管理 LLM 提供商、API Key 和 models.dev 目录。
---

CAB 维护一份来自 [models.dev](https://models.dev) 的实时 LLM 提供商与模型目录。仪表盘用于启用提供商、配置 API Key 并选择参与路由的模型。

## 提供商页面

每行展示：

- 提供商名称与 ID
- 启用/禁用状态
- 已配置的 API Key 与上游端点

**添加提供商：**

1. 打开 **提供商**。
2. 点击 **添加** 或展开目录条目。
3. 输入一个或多个 API Key（支持多 Key 轮换）。
4. 启用该提供商。

CAB 在请求时根据订阅状态和可用性选择首选 Key。

## 模型页面

**模型** 目录展示 models.dev 与 Artificial Analysis 同步的基准数据：

| 字段 | 含义 |
| ---- | ---- |
| **Coding index** | AA 编程基准分 |
| **Intelligence / Agentic** | 通用与 Agentic 能力分 |
| **Context window** | 最大输入 token |
| **Price** | 每百万 token 的输入/输出价格 |

可单独启用或禁用模型。只有 **已启用提供商上的已启用模型** 才参与路由。

## 订阅 vs 按量付费

CAB 区分订阅 Key 与按量付费 Key。路由策略会据此不同地权衡成本——例如 balanced 策略在订阅 Key 上可能更倾向高价值模型。

## 目录同步

CAB 在启动时和按需同步提供商与模型元数据。设置中还支持 **Artificial Analysis API Key** 以获取更丰富的基准数据（也可用环境变量 `ARTIFICIAL_ANALYSIS_API_KEY`）。

## 建议

- 至少启用两个不同价位区间的模型，auto 和 balanced 策略才有意义的选择空间。
- 禁用不需要的模型以缩小候选集、加快解析。
- 调优路由前先查看模型页面的基准分——intelligent 和 auto 策略依赖这些数据。
