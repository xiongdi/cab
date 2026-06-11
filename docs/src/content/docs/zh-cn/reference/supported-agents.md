---
title: 支持的 Agent
description: CAB 集成的编码 Agent 及其配置文件路径。
---

CAB v0.2.x 集成七个编码 Agent CLI。网关在 User-Agent 中识别各 Agent。

| Agent | User-Agent ID | 配置路径 |
| ----- | ------------- | -------- |
| Claude Code | `claude-code` | `~/.claude/settings.json` |
| Codex | `codex` | `~/.codex/config.toml` |
| OpenCode | `opencode` | `~/.config/opencode/opencode.json` |
| Hermes | `hermes` | `~/.hermes/config.yaml` |
| Kilo Code | `kilocode` | `~/.config/kilo/opencode.json` |
| OpenClaw | `openclaw` | `openclaw config` → `openclaw.json` |
| Pi | `pi` | `~/.pi/agent/models.json`、`~/.pi/agent/settings.json` |

## 网关端点

```
http://127.0.0.1:3125/v1
```

## 模式摘要

| 模式 | 配置变更 |
| ---- | -------- |
| 原生 | 无变更——CAB 从备份恢复原始配置 |
| 自动 | 网关 URL + Bearer 密钥 + 路由策略 |
| 手动 | 网关 URL + Bearer 密钥 + 注册所有已启用模型 |

详见 [Agent 模式](../../guides/agents/)。

## 配置备份

CAB 在修改 Agent 配置前创建备份（包括 Codex 的 `auth.json` 和 OpenAI 登录状态）。切回原生模式时恢复原始文件。
