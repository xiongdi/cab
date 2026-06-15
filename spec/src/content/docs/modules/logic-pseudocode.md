---
title: 逻辑细节与伪代码
description: 路由、Fallback 与 429 额度核心算法
chapter: modules
order: 2
---

## 请求画像（`build_request_profile`）

```
text ← extract_request_text(body)   // messages / input 拼接
message_count ← count_messages(body)
has_tools ← body.tools 或 body.functions 存在
profile ← classify_request(text, agent, message_count, has_tools)
  → TaskKind: Coding | Math | Agentic | General
  → complexity: 0.0 ~ 1.0
  → estimated_input_tokens
```

## 有效成本与性价比（`routing.rs`）

```
blended_input ← cache_read 存在 ?
  0.9 × cache_read + 0.1 × input : input

effective_cost ← blended_input × 10 + output    // BALANCED_INPUT_OUTPUT_RATIO = 10

capability_value_score(cap, input, output, cache_read):
  IF input 或 output 缺失 → return -∞
  raw ← effective_cost(input, output, cache_read)
  IF raw ≤ 0 → return +∞
  ELSE → return capability / raw
```

候选价格为 **端点价**（`RouteCandidate.input_cost` / `output_cost` / `cache_read_cost`），来自 models.dev 矩阵行。

## 能力分（`score_parts`）

```
primary_capability_loose(model, task):
  Coding   → coding_index ?? overall_intelligence
  Math     → math_index ?? overall_intelligence
  Agentic  → agentic_index ?? overall_intelligence
  General  → overall_intelligence

composite_capability(model, task):   // 四项 AA 指数齐全时
  Coding   → 0.55·coding + 0.22·overall + 0.13·agentic + 0.10·math
  Math     → 0.58·math + 0.24·overall + 0.10·coding + 0.08·agentic
  Agentic  → 0.42·agentic + 0.28·overall + 0.22·coding + 0.08·math
  General  → 0.45·overall + 0.22·coding + 0.18·math + 0.15·agentic
```

## 各策略 value / capability

| 策略 | capability | value |
| ---- | ---------- | ----- |
| balanced | primary_capability_loose | capability_value_score |
| auto | composite 或 primary | capability_value_score |
| cheapest | 0 | -effective_cost |
| intelligent | coding_index | capability（同左） |
| speed | output_speed_tps | capability（同左）；全无速度 → 降级 cheapest |

## Auto 能力门槛（`min_required_capability`）

```
min_required = floor + complexity × (ceiling - floor)

Coding:   floor=32, ceiling=88
Math:     floor=38, ceiling=92
Agentic:  floor=42, ceiling=95
General:  floor=24, ceiling=78

FILTER capability >= min_required
IF 结果为空 → 不过滤，对全量候选重新打分排序
```

## 统一排序（`score_route_candidates`）

```
FOR each routable (model, service_provider) with endpoint prices:
  计算 capability, value, endpoint_cost

SORT BY:
  1. value DESC
  2. capability DESC
  3. IF strategy == Speed: time_to_first_token ASC
  4. endpoint_cost ASC
  5. model.name ASC
  6. service_provider_id ASC
```

`cheapest` 的 value 为负成本，等价于按 effective_cost 升序。

## 路由页策略板（`strategy_board`）

```
profile ← build_request_profile(body, agent)
FOR strategy IN [auto, balanced, cheapest, intelligent, speed]:
  effective ← speed 且无速度数据 ? cheapest : strategy
  candidates ← rank_route_candidates_with_scores(全部端点候选, effective, profile)
RETURN { strategies: [{ id, display_strategy, task, complexity, candidates }] }
```

仪表盘 **路由** 页候选表仅展示此 API 结果，不在前端重复实现排序。

## 路由解析（`resolve_route`）

```
1. 查找 routes WHERE agent_pattern 匹配 agent
   → 使用 route.routing_strategy + primary/fallback 模型
2. ELSE IF requested_model 在 models 中
   → 直连该模型
3. ELSE IF requested_model 为 auto 或策略名
   → rank_route_candidates 取第一名
4. ELSE → NotFound
```

## Fallback 执行（`execute_with_fallback`）

```
candidates ← [primary] + fallback_models
FOR resolved IN candidates:
  keys ← ordered_api_keys(resolved.api_keys)
  endpoints ← resolved.endpoint_candidates
  FOR key IN keys:
    FOR endpoint IN endpoints:
      TRY proxy_request(endpoint, key, body)
      ON 429:
        quota_reset_at ← resolve_quota_reset_at(headers)
        mark_api_key_quota_reset(provider_id, key, quota_reset_at)
        CONTINUE next key
      ON success → RETURN response
RETURN last_error
```

## API Key 选择（`ordered_api_keys`）

```
RETURN api_keys 配置顺序中 enabled 且 NOT rate_limited 的 key
```

## Agent 配置写入（`apply_agent_config`）

```
SWITCH agent.id:
  claude-code / codex / opencode / hermes / kilocode / openclaw / pi
    → 写入对应配置中的 CAB base URL + gateway_key
  mode ∈ {native, auto, manual}
  ...
backup 原文件到 backups/*.cab-backup.{timestamp}
```

## state.json 原子写（`save_from_store`）

```
data ← read StoreData.agents + StoreData.routes
json ← serialize PersistedState { version: 1, agents, routes }
write ~/.cab/state.json.tmp
rename state.json.tmp → state.json
```

## Gateway 鉴权（`auth_middleware`）

```
IF NOT settings.auth_enabled → next.run(req)
token ← Authorization header Bearer value
IF token != settings.gateway_key → 401
ELSE next.run(req)
```

## JSONL 日志（`log_store::append`）

```
path ← ~/.cab/logs/requests-{today}.jsonl
append one JSON line per RequestLog
update in-memory ring buffer (max 500)
```

## 路由解释（`route_explainer::explain`）

```
steps ← empty
profile ← build_request_profile(body, agent)
strategy ← infer_strategy(agent, requested_model)
ranked ← rank_route_candidates_with_scores(..., strategy, profile)  // 前 10
resolved ← resolve_route(...)
RETURN { resolved, decision_steps: steps, ranked_candidates: ranked }
```
