---
title: Agents
description: Connect coding agents to CAB with Native, Auto, and Manual modes.
---

The **Agents** page is CAB's control center for coding agent integrations. Each supported agent can run in one of three modes.

## Modes

| Mode       | What happens                                                                                                                         |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| **Native** | Agent keeps its original config and talks directly to its default backend. CAB does not intervene.                                   |
| **Auto**   | CAB rewrites the agent config to point at the local gateway and binds a **routing strategy**. Every request is routed automatically. |
| **Manual** | CAB points the agent at the gateway and registers all enabled models. You choose the model inside the agent CLI.                     |

### When to use each mode

- **Native** — A/B testing against CAB, or using provider-specific features CAB doesn't proxy.
- **Auto** — Set-and-forget. Pick `balanced` or `auto` and let CAB handle model selection.
- **Manual** — You want explicit model control but still use CAB's gateway, auth, and protocol conversion.

## Per-agent integration

| Agent           | Config location                             | Auto mode                                                   | Manual mode                                           |
| --------------- | ------------------------------------------- | ----------------------------------------------------------- | ----------------------------------------------------- |
| **Claude Code** | `~/.claude/settings.json`                   | Rewrites gateway URL + Bearer key; strategy-driven routing  | Gateway discovery with `claude/cab/...` model aliases |
| **Codex**       | `~/.codex/config.toml`                      | Sets CAB provider + strategy; manages OAuth via `auth.json` | Lists all enabled models via `/v1/models`             |
| **OpenCode**    | `~/.config/opencode/opencode.json`          | Registers `cab/auto` strategy aliases                       | Writes all enabled models under cab provider          |
| **Hermes**      | `~/.hermes/config.yaml`                     | OpenAI-compatible mode + custom headers for agent ID        | Same gateway, model chosen in Hermes                  |
| **Kilo Code**   | `~/.config/kilo/opencode.json`              | OpenCode-format cab provider + strategy                     | All enabled models registered                         |
| **OpenClaw**    | `openclaw config` CLI                       | CAB as OpenAI-compatible provider, default `cab/auto`       | Default model set to chosen strategy or model         |
| **Pi**          | `~/.pi/agent/models.json` + `settings.json` | CAB provider + default strategy                             | Full model list in Ctrl+P picker                      |
| **Reasonix**    | `~/.reasonix/config.toml` + `.env`          | CAB provider entry + strategy as default model              | All enabled models listed                             |

CAB **backs up** existing agent configs before rewriting and **restores** them when you switch back to Native mode.

## Gateway endpoint

All CAB-managed modes use:

```
http://127.0.0.1:3125/v1
```

Bearer token: your **Gateway API Key** from Settings (injected automatically on save).

## Codex note

In Auto/Manual mode, CAB manages Codex authentication dynamically via `auth.json` using ChatGPT OAuth tokens — you don't need a separate `OPENAI_API_KEY` environment variable.

## Related

- [Supported agents reference](../../reference/supported-agents/) — full config paths
- [Quick Start](../../getting-started/quick-start/) — connect your first agent
- [Routing](../routing/) — strategy details
