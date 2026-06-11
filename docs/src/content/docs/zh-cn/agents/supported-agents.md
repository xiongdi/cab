---
title: 支持的编码代理
description: CAB 支持的 Agent 集成与配置文件路径。
---

CAB 可改写 Agent 配置，使编码 CLI 指向本地网关。在 **Agents** 页面配置模式：**Native**（绕过 CAB）、**Auto**（路由策略）、**Manual**（暴露所有已启用模型）。

| Agent       | 集成                               |
| ----------- | ---------------------------------- |
| Claude Code | `~/.claude/settings.json`          |
| Codex       | `~/.codex/config.toml`             |
| OpenCode    | `~/.config/opencode/opencode.json` |
| Hermes      | `~/.hermes/config.yaml`            |
| Kilo Code   | `~/.config/kilo/opencode.json`     |
| OpenClaw    | `openclaw config`                  |
| Pi          | `~/.pi/agent/models.json`          |

## 网关地址

将所有支持的 Agent 指向：

```
http://127.0.0.1:3125/v1
```

若 Agent 需要 Bearer API Key，请使用 CAB 设置中的网关令牌。
