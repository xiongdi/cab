---
title: 路由策略
description: CAB 内置路由策略与自定义路由规则。
---

CAB 决定每个网关请求由哪个模型和提供商处理。路由在网关层完成，随后才转发到上游。

## 解析顺序

1. **Agent 自动模式**——若 Agent 处于自动模式且绑定了策略，优先使用该策略。
2. **自定义路由规则**——路由页面中按 Agent User-Agent 模式匹配的规则。
3. **请求模型**——若客户端指定了目录中存在的模型 ID，直接使用。

使用 **路由 → 解释路由** 模拟请求，查看决策步骤和候选排序。路由页的各策略候选表由 `POST /api/routing/strategy-board` 返回，与网关实际排序共用 `cab-core` 同一套算法。

## 排序算法（权威定义）

实现位于 `crates/cab-core/src/routing.rs`。所有策略对 **可路由候选** `(模型, 服务商)` 打分；价格取 **端点定价**（`endpoint_input_cost` / `endpoint_output_cost` / `endpoint_cache_read_cost`），即 models.dev 上该服务商的真实单价，而非模型目录默认价。

### 公共公式

**混合输入价**（存在 `cache_read` 时）：

```
blended_input = 0.9 × cache_read + 0.1 × input
```

否则 `blended_input = input`。

**有效 token 成本**（美元 / 百万 token）。默认辅助函数使用 **10:1** 输入/输出比（`BALANCED_INPUT_OUTPUT_RATIO`）。Balanced / Auto 的性价比打分使用 **请求画像比例**（`estimated_input / estimated_output`，钳制在 0.5–50）：

```
effective_cost = blended_input × ratio + output
```

**性价比分数**（`auto` / `balanced` 主键；亦为 `intelligent` / `agentic` 的次键）：

```
若 input 与 output 已知且 effective_cost > 0:
  value = capability / effective_cost
若 input 与 output 已知且 effective_cost ≤ 0（已知免费）:
  value = +∞
若 input 或 output 缺失:
  value = -∞（排最后）
```

**任务主能力**（`balanced` / `auto` 打分与 auto 过滤使用）：

| 任务    | 主能力指数                                   |
| ------- | -------------------------------------------- |
| coding  | `coding_index`，否则 `overall_intelligence`  |
| math    | `math_index`，否则 `overall_intelligence`    |
| agentic | `agentic_index`，否则 `overall_intelligence` |
| general | `overall_intelligence`                       |

**请求画像**（`build_request_profile`）：从消息文本、Agent ID、tools 等推断 `task` 与 `complexity`（0.0–1.0）。

### 各策略排序键

每种策略保存正语义的 **主键**（`value`）与 **次键**（`capability`），比较方向因策略而异；再平局则模型名、服务商 ID。

| 策略            | 主键（`value`）                        | 次键（`capability`）        | 主键方向 | 次键方向 | 参与条件                          |
| --------------- | -------------------------------------- | --------------------------- | -------- | -------- | --------------------------------- |
| **auto**        | capability / effective_cost            | `overall_intelligence`      | DESC     | DESC     | 有任务主能力                      |
| **balanced**    | capability / effective_cost            | `overall_intelligence`      | DESC     | DESC     | 有任务主能力                      |
| **cheapest**    | `effective_cost`                       | `overall_intelligence`      | ASC      | DESC     | 始终（缺价沉底）                  |
| **intelligent** | `coding_index`                         | capability / effective_cost | DESC     | DESC     | 有 `coding_index`                 |
| **agentic**     | `agentic_index`                        | capability / effective_cost | DESC     | DESC     | 有 `agentic_index`                |
| **speed**       | `TTFT + 1000 / output_speed_tps`（秒） | `effective_cost`            | ASC      | ASC      | 有 AA 速度数据；否则降级 cheapest |

**Auto 过滤**（排序前；若为空则回退）：

1. **能力门槛**：`min_required = floor + complexity × (ceiling - floor)`

| 任务    | floor | ceiling |
| ------- | ----- | ------- |
| coding  | 32    | 88      |
| math    | 38    | 92      |
| agentic | 42    | 95      |
| general | 24    | 78      |

仅主能力 ≥ `min_required` 的候选保留。

2. **成本上限**（当 `complexity < 0.6`）：去掉超过任务基准最大有效成本的候选；若清空则回退到能力过滤后的集合。

## 内置策略

可作为 Agent 策略或路由目标：

### Auto（自动智能选择）

解析请求画像 → 应用能力门槛（及可选成本上限）→ 按 **性价比** 与统一平局规则排序。

适合：混合工作负载，希望 CAB 按请求自适应。

### Balanced（平衡推荐）

按 **任务主能力 / 有效成本** 排序，不随请求复杂度抬门槛。

适合：日常编程，兼顾成本。推荐作为默认策略。

### Intelligent（代码能力最强）

按 **AA coding index** 降序；同分则更好性价比，再按模型名与服务商。

适合：高难度 Bug 修复、复杂重构、架构设计。

### Agentic（智能体优先）

按 **AA agentic index** 降序；同分则更好性价比，再按模型名与服务商。

适合：工具密集、多步 Agent 工作流。

### Price（价格最惠，`cheapest`）

按 **有效成本** 从低到高；同价则更高 overall intelligence，再按模型名、服务商。

适合：预算敏感场景和简单任务。

### Speed（速度优先）

按 **1000 token 总响应时间** 升序：`TTFT + 1000 / tps`。同速则更低有效成本。无速度数据者不可路由；若全部无数据则降级为 **Price**。

适合：交互式编码、快速补全、对延迟敏感的工作流。

## 自定义路由规则

**路由** 页面可定义：

- **Agent 模式**——User-Agent 通配匹配（如 `codex`、`claude-code`、`pi`）
- **路由策略**——内置策略或指定模型
- **降级链**——主模型不可用时的备选

自定义规则会覆盖匹配 Agent 的默认解析。

## 降级

主模型或端点失败时，CAB 尝试备选候选（内置策略最多两个）。端点选择优先原生协议匹配，再回退到协议转换。

## 相关

- [Agent 模式](../agents/)
- [API 参考](../../reference/api/) — `POST /api/routing/explain` 与 `POST /api/routing/strategy-board`
