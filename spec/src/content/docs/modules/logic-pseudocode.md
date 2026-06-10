---
title: 逻辑细节与伪代码
description: 路由、Fallback 与订阅额度核心算法
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

## Auto 策略排序（`rank_models`）

```
FOR each enabled model:
  routing_cost ← effective_routing_cost(model, subscribed_provider_ids)
    IF provider 有可用订阅 Key → routing_cost = MIN_COST_EPSILON (0.001)
    ELSE routing_cost = input×3 + output
  capability ← composite_capability(model, profile.task)
  value ← capability / routing_cost

IF strategy == Auto:
  min_required ← min_required_capability(profile)  // 随 complexity 升高
  FILTER scored WHERE capability >= min_required
  IF empty → 回退为全量模型再排序

SORT BY value DESC → 返回模型列表
```

## 路由解析（`resolve_route`）

```
1. 查找 routes WHERE agent_pattern 匹配 agent
   → 使用 route.routing_strategy + primary/fallback 模型
2. ELSE IF requested_model 在 models 中
   → 直连该模型
3. ELSE IF requested_model 为 auto 或策略名
   → rank_models 取第一名
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

## 订阅 Key 选择（`ordered_api_keys`）

```
keys_sub ← enabled + subscribed + NOT rate_limited
keys_pay ← enabled + NOT subscribed + NOT rate_limited
RETURN keys_sub ++ keys_pay
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
resolved ← resolve_route_with_trace(catalog, agent, model, body, steps)
candidates ← rank_models(...) with summaries
RETURN { resolved, decision_steps: steps, ranked_candidates: candidates }
```
