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

CAB selects the preferred key at request time based on subscription status and availability.

## Models page

The **Models** catalog shows benchmark data synced from models.dev and Artificial Analysis:

| Field | Meaning |
| ----- | ------- |
| **Coding index** | AA coding benchmark score |
| **Intelligence / Agentic** | General and agentic capability scores |
| **Context window** | Max input tokens |
| **Price** | Input and output cost per million tokens |

Enable or disable individual models. Only **enabled models on enabled providers** are eligible for routing.

## Subscription vs. pay-as-you-go

CAB tracks whether a provider key is subscription-based or pay-as-you-go. Routing strategies weigh cost differently depending on key type — for example, the balanced strategy may prefer high-value models on subscription keys.

## Catalog sync

CAB syncs provider and model metadata on startup and on demand. Settings also support an **Artificial Analysis API key** for richer benchmark data (`ARTIFICIAL_ANALYSIS_API_KEY` env var as fallback).

## Tips

- Enable at least two models across different price tiers so strategies like **auto** and **balanced** have meaningful choices.
- Disable models you never want routed to — this shrinks the candidate set and speeds resolution.
- Check the Models page before tuning routes; benchmark scores drive intelligent and auto strategies.
