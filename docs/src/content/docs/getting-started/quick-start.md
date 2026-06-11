---
title: Quick Start
description: Go from install to your first routed agent request in five minutes.
---

This guide walks through the core workflow: install → configure a provider → connect a coding agent.

## 1. Launch CAB

Open the CAB desktop app. The local gateway starts automatically on port **3125** (configurable in Settings).

Default endpoint:

```
http://127.0.0.1:3125/v1
```

## 2. Add a provider

1. Open **Providers** in the sidebar.
2. Wait for the models.dev catalog to sync (or click refresh).
3. Expand a provider row and add at least one **API key**.
4. Enable the provider and the models you want CAB to route to.

Without an enabled provider + model, routing has nowhere to send requests.

## 3. Copy your gateway key

1. Open **Settings**.
2. Copy the **Gateway API Key** — agents in Auto/Manual mode use this as the Bearer token.
3. Auth is enabled by default (`auth_enabled: true`). Every gateway request needs:

   ```
   Authorization: Bearer <gateway_key>
   ```

   Agents switched via CAB receive this automatically.

## 4. Connect a coding agent

1. Open **Agents**.
2. Pick an agent (e.g. Codex or Claude Code).
3. Choose a mode:
   - **Auto** — CAB rewrites the agent config and applies a routing strategy (`balanced` is a good default).
   - **Manual** — CAB registers all enabled models; you pick in the agent CLI.
   - **Native** — bypass CAB entirely (useful for comparison).
4. Click **Save**. CAB backs up and rewrites the agent's config file.

See [Agent modes](../../guides/agents/) for per-agent details.

## 5. Send a test request

Run your agent CLI as usual. CAB intercepts the request, ranks models, and forwards to the best match.

Check **Logs** in the dashboard to confirm the routed provider, model, token usage, and latency.

## 6. Tune routing (optional)

- Change the agent's strategy in **Agents** (auto / balanced / intelligent / price / speed).
- Create custom rules in **Routes** with agent patterns and fallback chains.
- Use **Routes → Explain routing** to preview how a prompt would be resolved.

## Next steps

- [Providers & models](../../guides/providers-and-models/) — catalog sync and model selection
- [Routing strategies](../../guides/routing/) — built-in strategies explained
- [Supported agents](../../reference/supported-agents/) — config file paths per agent
