---
title: Routing
description: Built-in routing strategies and custom route rules in CAB.
---

CAB decides which model and provider handle each gateway request. Routing happens in the gateway layer before the request is forwarded upstream.

## Resolution order

1. **Agent auto mode** — if the agent is in Auto mode with a configured strategy, that strategy applies first.
2. **Custom route rules** — glob-matched rules in the Routes page (by agent User-Agent pattern).
3. **Requested model** — if the client sends a specific model ID that exists in the catalog, use it directly.

Use **Routes → Explain routing** to simulate a request and inspect the decision steps and ranked candidates.

## Built-in strategies

These are available as agent strategies and as route targets:

### Auto

Analyzes each request to detect task type (coding / math / agentic / general) and complexity. Scores models with weighted AA indices, raises the capability floor for harder prompts, then ranks by **capability / effective cost** where effective cost = input×3 + output×1.

Best for: mixed workloads where you want CAB to adapt per request.

### Balanced

Ranks by **primary task capability / (input×3 + output×1)**. Balances flagship capability against realistic agent token ratios.

Best for: everyday coding with sensible cost control. A good default.

### Intelligent

Routes to the highest **AA coding index** among enabled models. Ties broken by lower cost.

Best for: hard debugging, complex refactors, architecture work.

### Price

Sorts all enabled models by total token cost (input + output), lowest first. Filters invalid negative prices.

Best for: budget-constrained workflows and simple tasks.

### Speed

Routes to the highest **AA median output speed** (`median_output_tokens_per_second`) among enabled models. Ties break on lower time-to-first-token, then lower effective cost. Models without AA speed data are deprioritized; if none have data, falls back to **Price**.

Best for: interactive coding, quick completions, and latency-sensitive workflows.

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
- [API reference](../../reference/api/) — `POST /api/routing/explain` endpoint
