---
title: Agent 配置
description: 以原生、自动、手动三种模式将编码 Agent 接入 CAB。
---

**Agent** 页面是 CAB 管理编码 Agent 集成的控制中心。每个支持的 Agent 可运行以下三种模式之一。

## 工作模式

| 模式     | 行为                                                            |
| -------- | --------------------------------------------------------------- |
| **原生** | Agent 保持原始配置，直接访问默认后端，CAB 不干预。              |
| **自动** | CAB 改写配置指向本地网关并绑定 **路由策略**，每次请求自动选模。 |
| **手动** | CAB 指向网关并注册所有已启用模型，在 Agent CLI 内自行选择。     |

### 如何选择

- **原生**——与 CAB 对比测试，或使用 CAB 未代理的提供商特性。
- **自动**——一劳永逸。选 `balanced` 或 `auto`，让 CAB 自动选模。
- **手动**——需要明确控制模型，但仍使用 CAB 的网关、认证和协议转换。

## 各 Agent 对接方式

| Agent           | 配置位置                                    | 自动模式                                           | 手动模式                                  |
| --------------- | ------------------------------------------- | -------------------------------------------------- | ----------------------------------------- |
| **Claude Code** | `~/.claude/settings.json`                   | 改写网关 URL + Bearer；策略驱动路由                | Gateway discovery + `claude/cab/...` 别名 |
| **Codex**       | `~/.codex/config.toml`                      | 配置 CAB 提供商与策略；通过 `auth.json` 管理 OAuth | 通过 `/v1/models` 列出所有已启用模型      |
| **OpenCode**    | `~/.config/opencode/opencode.json`          | 注册 `cab/auto` 等策略别名                         | 将所有已启用模型写入 cab provider         |
| **Hermes**      | `~/.hermes/config.yaml`                     | OpenAI 兼容模式 + 自定义请求头                     | 同一网关，在 Hermes 内选模型              |
| **Kilo Code**   | `~/.config/kilo/opencode.json`              | OpenCode 格式 cab provider + 策略                  | 注册全部已启用模型                        |
| **OpenClaw**    | `openclaw config` → `openclaw.json`         | CAB 作为 OpenAI 兼容提供商，默认 `cab/auto`        | 默认模型设为所选策略或模型                |
| **Pi**          | `~/.pi/agent/models.json` + `settings.json` | CAB provider + 默认策略                            | Ctrl+P 模型选择器中的完整列表             |

CAB 在改写前 **备份** 现有配置，切回原生模式时 **恢复**。

## 网关端点

所有 CAB 管理模式使用：

```
http://127.0.0.1:3125/v1
```

Bearer 令牌：设置中的 **Gateway API Key**（保存时自动注入）。

## Codex 说明

自动/手动模式下，CAB 通过 `auth.json` 动态管理 Codex 认证（ChatGPT OAuth），无需单独配置 `OPENAI_API_KEY` 环境变量。

## 相关

- [支持的 Agent 参考](../../reference/supported-agents/)
- [快速开始](../../getting-started/quick-start/)
- [路由策略](../routing/)
