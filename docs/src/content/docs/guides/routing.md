---
title: Routing
description: Built-in routing strategies and custom route rules in CAB.
---

CAB decides which model and provider handle each gateway request. Routing happens in the gateway layer before the request is forwarded upstream.

## Resolution order

1. **Agent auto mode** — if the agent is in Auto mode with a configured strategy, that strategy applies first.
2. **Custom route rules** — glob-matched rules in the Routes page (by agent User-Agent pattern).
3. **Requested model** — if the client sends a specific model ID that exists in the catalog, use it directly.

Use **Routes → Explain routing** to simulate a request and inspect decision steps and ranked candidates. Strategy candidate tables on the Routes page come from `POST /api/routing/strategy-board` and use the same `cab-core` ranking code as the gateway.

## Ranking algorithm (authoritative)

Implemented in `crates/cab-core/src/routing.rs`. Every strategy scores **routable candidates** `(model, service_provider)`. Prices use **endpoint pricing** (`endpoint_input_cost`, `endpoint_output_cost`, `endpoint_cache_read_cost`) from models.dev for that provider—not the model’s catalog default alone.

### Shared formulas

**Blended input** (when `cache_read` exists):

```
blended_input = 0.9 × cache_read + 0.1 × input
```

Otherwise `blended_input = input`.

**Effective token cost** (10:1 input/output weighting for coding agents; USD per 1M tokens):

```
effective_cost = blended_input × 10 + output
```

**Value score** (`auto` / `balanced`):

```
If input and output are known and effective_cost > 0:
  value = capability / effective_cost
If input and output are known and effective_cost ≤ 0 (known free):
  value = +∞
If input or output is missing:
  value = -∞ (sorted last)
```

**Primary task capability** (`balanced` and `auto` fallback):

| Task | Primary capability |
| ---- | ------------------ |
| coding | `coding_index`, else `overall_intelligence` |
| math | `math_index`, else `overall_intelligence` |
| agentic | `agentic_index`, else `overall_intelligence` |
| general | `overall_intelligence` |

**Composite capability** (`auto` only, when all four AA indices exist):

| Task | Weighted blend |
| ---- | -------------- |
| coding | 0.55×coding + 0.22×overall + 0.13×agentic + 0.10×math |
| math | 0.58×math + 0.24×overall + 0.10×coding + 0.08×agentic |
| agentic | 0.42×agentic + 0.28×overall + 0.22×coding + 0.08×math |
| general | 0.45×overall + 0.22×coding + 0.18×math + 0.15×agentic |

**Request profile** (`build_request_profile`): infers `task` and `complexity` (0.0–1.0) from message text, agent id, tools, etc.

### Unified tie-break (except `cheapest`)

After sorting by **value descending**, equal values break in order:

1. **capability descending** (stronger model wins at the same value; at +∞ this puts M3 ahead of M2.7 when coding index is higher)
2. **speed only**: time-to-first-token **ascending**
3. **effective_cost ascending**
4. **model id ascending**
5. **service_provider_id ascending**

`cheapest` sorts by value (= negative effective cost), i.e. lowest cost first, then model id, then provider id.

### Per-strategy scoring

| Strategy | capability | value | Eligibility |
| -------- | ---------- | ----- | ----------- |
| **balanced** | primary task capability | capability / effective_cost (or +∞) | has primary capability |
| **auto** | composite or primary | same as balanced | capability floor by complexity, see below |
| **cheapest** | 0 | `-effective_cost` | known input & output |
| **intelligent** | `coding_index` | same as capability | has `coding_index` |
| **speed** | `output_speed_tps` | same as capability | has AA speed; else fallback to **cheapest** |

**Auto capability floor** (filter before rank; if empty, rerank all):

```
min_required = floor + complexity × (ceiling - floor)
```

| Task | floor | ceiling |
| ---- | ----- | ------- |
| coding | 32 | 88 |
| math | 38 | 92 |
| agentic | 42 | 95 |
| general | 24 | 78 |

Only candidates with `capability ≥ min_required` are ranked; harder prompts skew toward flagship models.

## Built-in strategies

Available as agent strategies and route targets:

### Auto

Build request profile → score capability → apply complexity floor → rank by **value** and unified tie-breaks.

Best for: mixed workloads where CAB should adapt per request.

### Balanced

Rank by **primary task capability / effective cost** (10:1 weighting + cache-read blend). No complexity floor.

Best for: everyday coding with sensible cost control. A good default.

### Intelligent

Highest **AA coding index** first; ties → lower cost → model id → provider.

Best for: hard debugging, complex refactors, architecture work.

### Price (`cheapest`)

Lowest **effective cost** first; ties → model id → provider.

Best for: budget workflows and simple tasks.

### Speed

Highest **AA output speed (tokens/s)** first; ties → lower TTFT → lower effective cost. Models without speed data sink; if none have data, fallback to **Price**.

Best for: interactive coding, quick completions, latency-sensitive workflows.

## Custom route rules

The **Routes** page lets you define rules with:

- **Agent pattern** — glob match on the agent User-Agent (e.g. `codex`, `claude-code`, `pi`)
- **Routing strategy** — one of the built-in strategies or a specific model
- **Fallback chain** — alternate models if the primary is unavailable

Custom rules override the default resolution for matching agents.

## Fallback

When the primary model or endpoint fails, CAB tries fallback candidates (up to two for built-in strategies). Endpoint selection prefers native protocol matches, then falls back to protocol conversion.

## Related

- [Agent modes](../agents/) — how strategies bind to agents in Auto mode
- [API reference](../../reference/api/) — `POST /api/routing/explain` and `POST /api/routing/strategy-board`
