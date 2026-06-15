---
title: Providers & Models
description: Manage LLM providers, API keys, and the models.dev catalog in CAB.
---

CAB maintains a live catalog of LLM providers and models, synced from [models.dev](https://models.dev). The dashboard lets you enable providers, attach API keys, and choose which models participate in routing.

## Providers page

Each provider row shows:

- Provider name and ID
- Enabled/disabled status
- Configured API keys and upstream endpoints

**To add a provider:**

1. Open **Providers**.
2. Click **Add** or expand a catalog entry.
3. Enter one or more API keys (CAB supports multiple keys per provider for rotation).
4. Enable the provider.

CAB selects keys in configuration order, skipping keys in 429 cooldown.

## Models page

The **Models** catalog shows benchmark data synced from models.dev and Artificial Analysis:

| Field | Meaning |
| ----- | ------- |
| **Coding index** | AA coding benchmark score |
| **Intelligence / Agentic** | General and agentic capability scores |
| **Context window** | Max input tokens |
| **Price** | Input and output cost per million tokens |

Enable or disable individual models. Only **enabled models on enabled providers** are eligible for routing.

## Multiple providers and billing

models.dev lists the same model under different providers with distinct endpoint prices (for example pay-as-you-go `minimax` vs plan `minimax-cn-coding-plan`). **Enable the models.dev provider that matches your billing mode** and attach keys there; routing ranks by endpoint price and strategy scores. CAB no longer uses a per-key “subscription vs pay-as-you-go” flag.

## Catalog sync

CAB syncs provider and model metadata on startup and on demand. Settings also support an **Artificial Analysis API key** for richer benchmark data (`ARTIFICIAL_ANALYSIS_API_KEY` env var as fallback).

## Tips

- Enable at least two models across different price tiers so strategies like **auto** and **balanced** have meaningful choices.
- Disable models you never want routed to — this shrinks the candidate set and speeds resolution.
- Check the Models page before tuning routes; benchmark scores drive intelligent and auto strategies.
