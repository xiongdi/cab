---
title: Supported coding agents
description: Agent integrations and configuration paths supported by CAB.
---

CAB can rewrite agent configs so your coding CLI points at the local gateway. Configure modes in the **Agents** page: **Native** (bypass CAB), **Auto** (routing strategy), **Manual** (expose all enabled models).

| Agent       | Integration                        |
| ----------- | ---------------------------------- |
| Claude Code | `~/.claude/settings.json`          |
| Codex       | `~/.codex/config.toml`             |
| OpenCode    | `~/.config/opencode/opencode.json` |
| Hermes      | `~/.hermes/config.yaml`            |
| Kilo Code   | `~/.config/kilo/opencode.json`     |
| OpenClaw    | `openclaw config`                  |
| Pi          | `~/.pi/agent/models.json`          |

## Gateway endpoint

Point every supported agent at:

```
http://127.0.0.1:3125/v1
```

Use the gateway token from CAB settings when the agent requires a Bearer API key.
