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

**混合输入价**（有 `cache_read` 时）：

```
blended_input = 0.9 × cache_read + 0.1 × input
```

无 `cache_read` 时 `blended_input = input`。

**有效 Token 成本**（按 coding agent 典型 10:1 输入/输出用量加权，单位：美元 / 百万 token）：

```
effective_cost = blended_input × 10 + output
```

**性价比（value）**（用于 `auto` / `balanced`）：

```
若 input、output 均已知且 effective_cost > 0：
  value = capability / effective_cost
若 input、output 均已知且 effective_cost ≤ 0（已知免费）：
  value = +∞
若 input 或 output 缺失：
  value = -∞（排在队尾）
```

**任务主能力指数**（`balanced` 与 `auto` 缺省分项时使用）：

| 任务类型 | 主能力 |
| -------- | ------ |
| coding | `coding_index`，否则 `overall_intelligence` |
| math | `math_index`，否则 `overall_intelligence` |
| agentic | `agentic_index`，否则 `overall_intelligence` |
| general | `overall_intelligence` |

**综合能力指数**（仅 `auto`，且四项 AA 指数齐全时）：

| 任务 | 加权公式 |
| ---- | -------- |
| coding | 0.55×coding + 0.22×overall + 0.13×agentic + 0.10×math |
| math | 0.58×math + 0.24×overall + 0.10×coding + 0.08×agentic |
| agentic | 0.42×agentic + 0.28×overall + 0.22×coding + 0.08×math |
| general | 0.45×overall + 0.22×coding + 0.18×math + 0.15×agentic |

**请求画像**（`build_request_profile`）：从消息文本、Agent ID、是否带 tools 等推断 `task` 与 `complexity`（0.0～1.0）。

### 统一 tie-break（除 `cheapest` 外）

候选按 **value 降序** 排列后，若 value 相同，依次比较：

1. **capability 降序**（同性价比时能力更强者优先；`∞` 时因此 M3 会排在 M2.7 前）
2. **speed 策略额外**：首 token 延迟 TTFT **升序**
3. **effective_cost 升序**（更便宜者优先）
4. **模型 ID 字典序升序**
5. **服务商 ID 字典序升序**

`cheapest` 按 value（即负有效成本）升序等价于 effective_cost 升序，平局再比模型名、服务商 ID。

### 各策略评分规则

| 策略 | capability | value | 参与条件 |
| ---- | ---------- | ----- | -------- |
| **balanced** | 任务主能力指数 | capability / effective_cost（或 ∞） | 有主能力指数 |
| **auto** | 综合能力或主能力 | 同上 | 先按复杂度过滤能力门槛，见下 |
| **cheapest** | 0 | `-effective_cost` | 有已知 input/output |
| **intelligent** | `coding_index` | 同 capability | 有 `coding_index` |
| **speed** | `output_speed_tps` | 同 capability | 有 AA 输出速度；全无则降级 **cheapest** |

**auto 能力门槛**（过滤后再排序；若过滤后为空则回退全量）：

```
min_required = floor + complexity × (ceiling - floor)
```

| 任务 | floor | ceiling |
| ---- | ----- | ------- |
| coding | 32 | 88 |
| math | 38 | 92 |
| agentic | 42 | 95 |
| general | 24 | 78 |

仅 `capability ≥ min_required` 的候选进入排序；复杂请求倾向旗舰，简单请求允许更便宜模型。

## 内置策略

可作为 Agent 策略或路由目标：

### Auto（自动智能选择）

解析请求画像 → 计算能力分 → 应用复杂度门槛 → 按 **性价比** 与统一 tie-break 排序。

适合：混合工作负载，希望 CAB 按请求自适应。

### Balanced（平衡推荐）

按 **任务主能力 / 有效成本** 排序（10:1 加权 + 缓存读价混合），不随请求复杂度抬门槛。

适合：日常编程，兼顾成本。推荐作为默认策略。

### Intelligent（代码能力最强）

按 **AA coding index** 降序；同分则更便宜、再按模型名与服务商。

适合：高难度 Bug 修复、复杂重构、架构设计。

### Price（价格最惠，`cheapest`）

按 **有效成本** 从低到高；同价按模型名、服务商。

适合：预算敏感场景和简单任务。

### Speed（速度优先）

按 **AA 输出速度（tokens/s）** 降序；同速则更低 TTFT、更低有效成本。无速度数据者排后；若全部无数据则降级为 **Price**。

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
- [API 参考](../../reference/api/)
