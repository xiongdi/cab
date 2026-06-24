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

capability_value_score(cap, input, output, cache_read) →
  cap / raw_effective_cost(input, output, cache_read)
  （raw ≤ 0 → +∞；input/output 任一缺失 → -∞）
```

## 各策略主 / 次排序键

`value` / `capability` 字段直接存**正数语义值**，比较器方向按策略不同：

| 策略                         | value（正数）                                            | capability（正数）            | value 方向 | capability 方向 |
| ---------------------------- | -------------------------------------------------------- | ----------------------------- | ---------- | --------------- |
| balanced（平衡策略）         | 性价比 capability_value_score                            | 智能指数 overall_intelligence | DESC       | DESC            |
| auto                         | 同 balanced（Auto = Balanced + capability floor filter） | 同上                          | DESC       | DESC            |
| cheapest / price（价格策略） | effective_cost（USD / Mtok）                             | 智能指数 overall_intelligence | ASC        | DESC            |
| intelligent（代码能力策略）  | coding_index                                             | 性价比 capability_value_score | DESC       | DESC            |
| speed（速度策略）            | total_response_time（秒）                                | effective_cost（USD / Mtok）  | ASC        | ASC             |

`Speed` 主键用 AA 风格的「Total Response Time for N Output Tokens」指标
（`TTFT + N / tps`），同时考虑首 token 延迟与稳态吞吐，比单纯 tps 更贴近用户对「速度」的体感。
`OUTPUT_TOKENS_FOR_SPEED_RANKING = 1000` 使数值更贴近真实编码场景。value 是正秒数（越小越快），
capability 是正成本（越小越好）。

主排序键缺失 ⇒ `value = +∞`（ASC 策略）或 `-∞`（DESC 策略），始终沉底。
次排序键缺失 ⇒ `capability = -∞`，永不赢次级 tiebreak。

`Speed` 整池没有速度数据时降级为 `Cheapest`。

## Auto 能力门槛（`min_required_capability`）

```
min_required = floor + complexity × (ceiling - floor)

Coding:   floor=32, ceiling=88
Math:     floor=38, ceiling=92
Agentic:  floor=42, ceiling=95
General:  floor=24, ceiling=78

FILTER primary_capability_loose(model, task) >= min_required
IF 结果为空 → 不过滤，对全量候选重新打分排序
```

## 统一排序（`score_route_candidates`）

```
FOR each routable (model, service_provider) with endpoint prices:
  计算 primary_score, secondary_score

SORT BY:
  1. primary_score   <direction per strategy>
  2. secondary_score <direction per strategy>
  3. model.name      ASC
  4. service_provider_id ASC
```

每个策略的 primary / secondary 由 `score_parts` 写入 `value` / `capability` 字段，
值为正数语义值。比较器方向按策略不同：balanced / auto / intelligent 用 DESC，
cheapest / speed 用 ASC。`/api/routing/explain` 直接读取这两个字段渲染给前端，
配合 `formatStrategyMetric` / `formatExplainValue` 加上单位后缀（`s`、`$/Mtok`）。

## 路由页策略板（`strategy_board`）

```
profile ← build_request_profile(body, agent)
FOR strategy IN [auto, balanced, cheapest, intelligent, speed, agentic]:
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
