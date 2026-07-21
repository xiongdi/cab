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

**Effective token cost** (USD per 1M tokens). Default helpers use a **10:1** input/output ratio (`BALANCED_INPUT_OUTPUT_RATIO`). Balanced / Auto value scoring uses a **request-profile ratio** (`estimated_input / estimated_output`, clamped 0.5–50):

```
effective_cost = blended_input × ratio + output
```

**Value score** (`auto` / `balanced` primary; also secondary for `intelligent` / `agentic`):

```
If input and output are known and effective_cost > 0:
  value = capability / effective_cost
If input and output are known and effective_cost ≤ 0 (known free):
  value = +∞
If input or output is missing:
  value = -∞ (sorted last)
```

**Primary task capability** (used by `balanced` / `auto` scoring and by auto filters):

| Task    | Primary capability                           |
| ------- | -------------------------------------------- |
| coding  | `coding_index`, else `overall_intelligence`  |
| math    | `math_index`, else `overall_intelligence`    |
| agentic | `agentic_index`, else `overall_intelligence` |
| general | `overall_intelligence`                       |

**Request profile** (`build_request_profile`): infers `task` and `complexity` (0.0–1.0) from message text, agent id, tools, etc.

### Sort keys (per strategy)

Each strategy stores a positive semantic **primary** (`value`) and **secondary** (`capability`). Comparator direction is per-strategy. Ties then break on model name, then service provider id.

| Strategy        | Primary (`value`)                          | Secondary (`capability`)    | Primary dir | Secondary dir | Eligibility                        |
| --------------- | ------------------------------------------ | --------------------------- | ----------- | ------------- | ---------------------------------- |
| **auto**        | capability / effective_cost                | `overall_intelligence`      | DESC        | DESC          | task capability available          |
| **balanced**    | capability / effective_cost                | `overall_intelligence`      | DESC        | DESC          | task capability available          |
| **cheapest**    | `effective_cost`                           | `overall_intelligence`      | ASC         | DESC          | always (missing cost → sink)       |
| **intelligent** | `coding_index`                             | capability / effective_cost | DESC        | DESC          | has `coding_index`                 |
| **agentic**     | `agentic_index`                            | capability / effective_cost | DESC        | DESC          | has `agentic_index`                |
| **speed**       | `TTFT + 1000 / output_speed_tps` (seconds) | `effective_cost`            | ASC         | ASC           | has AA speed data; else → cheapest |

**Auto filters** (before rank; if empty, fall back):

1. **Capability floor**: `min_required = floor + complexity × (ceiling - floor)`

| Task    | floor | ceiling |
| ------- | ----- | ------- |
| coding  | 32    | 88      |
| math    | 38    | 92      |
| agentic | 42    | 95      |
| general | 24    | 78      |

Only candidates with primary capability ≥ `min_required` remain.

2. **Cost ceiling** (when `complexity < 0.6`): drop candidates above a task-based max effective cost; if that empties the pool, revert to the capability-filtered set.

## Built-in strategies

Available as agent strategies and route targets:

### Auto

Build request profile → apply capability floor (+ optional cost ceiling) → rank by **value** (capability / cost) with unified tie-breaks.

Best for: mixed workloads where CAB should adapt per request.

### Balanced

Rank by **primary task capability / effective cost**. No complexity floor.

Best for: everyday coding with sensible cost control. A good default.

### Intelligent

Highest **AA coding index** first; ties → better cost-performance → model id → provider.

Best for: hard debugging, complex refactors, architecture work.

### Agentic

Highest **AA agentic index** first; ties → better cost-performance → model id → provider.

Best for: tool-heavy / multi-step agent workflows.

### Price (`cheapest`)

Lowest **effective cost** first; ties → higher overall intelligence → model id → provider.

Best for: budget workflows and simple tasks.

### Speed

Lowest **total response time** for 1000 output tokens: `TTFT + 1000 / tps`. Ties → lower effective cost. Models without speed data are unroutable; if none have data, fallback to **Price**.

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
