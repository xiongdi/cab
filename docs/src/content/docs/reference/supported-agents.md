---
title: Supported Agents
description: Coding agents integrated with CAB and their configuration paths.
---

CAB integrates with four coding agent CLIs. Each agent is identified by its User-Agent string at the gateway.

| Agent       | User-Agent ID | Config path                        |
| ----------- | ------------- | ---------------------------------- |
| Claude Code | `claude-code` | `~/.claude/settings.json`          |
| Codex       | `codex`       | `~/.codex/config.toml`             |
| OpenCode    | `opencode`    | `~/.config/opencode/opencode.json` |
| Grok Build  | `grok-build`  | `~/.grok/config.toml`              |

## Gateway endpoint

```
http://127.0.0.1:3125/v1
```

## Mode summary

| Mode   | Config change                                            |
| ------ | -------------------------------------------------------- |
| Native | No change — CAB restores previous config from backup     |
| Auto   | Gateway URL + Bearer key + routing strategy              |
| Manual | Gateway URL + Bearer key + all enabled models registered |

See [Agent modes](../../guides/agents/) for per-agent behavior.

## Config backup

CAB creates backups before modifying agent configs (notably Codex `auth.json` and OpenAI login state). Switching back to Native mode restores the original files.
