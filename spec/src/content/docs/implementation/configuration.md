---
title: 配置与密钥管理
description: CAB 配置文件、路径与密钥存储
chapter: implementation
order: 4
---

## 配置文件

| 文件                                      | 加载方              | 角色                                              | 内容                                                                                                                 |
| ----------------------------------------- | ------------------- | ------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `cab.toml`                                | `CabConfig::load()` | **系统引导**（启动时读一次，不可通过 API 修改）   | `gateway.host`（默认 127.0.0.1）、`gateway.port`（首次安装写入 settings.json 的种子值）                              |
| `~/.cab/settings.json`                    | `cab-db::settings`  | **用户运行时**（可通过 `PUT /api/settings` 修改） | `gateway_port`（运行时端口）、`gateway_key`、`auth_enabled`、`log_retention_days`、`providers`、`models`、`api_keys` |
| `config/provider-endpoints.defaults.json` | `provider_defaults` | 静态默认                                          | 提供商默认端点                                                                                                       |
| `config/aa-model-map.json`                | `benchmark_catalog` | 静态默认                                          | AA 模型名映射                                                                                                        |

### 端口优先级链

`settings.json gateway_port`（运行时，API 可编辑）→ `cab.toml [gateway] port`（引导默认）→ 硬编码 `3125`。

- 首次安装：`cab.toml` 的 port 写入 `settings.json`，之后运行时始终读 `settings.json`
- 用户通过 Settings 页面改端口 → 写入 `settings.json`，重启后生效
- `cab.toml` 的 port 仅在 `settings.json` 加载失败时作为 fallback

### Host vs Port 分工

| 配置项         | 来源            | 可通过 API 修改 | 说明                                   |
| -------------- | --------------- | --------------- | -------------------------------------- |
| `gateway.host` | `cab.toml`      | 否              | 系统级绑定地址，始终从 `cab.toml` 读取 |
| `gateway.port` | `settings.json` | 是              | 运行时端口，首次安装从 `cab.toml` 种子 |

## 运行时路径

| 路径                         | 用途                                       |
| ---------------------------- | ------------------------------------------ |
| `~/.cab/settings.json`       | 用户设置持久化                             |
| `~/.cab/catalog/`            | models.dev / AA 缓存                       |
| `~/.claude/settings.json` 等 | Agent 配置（由 `apply_agent_config` 写入） |

## 密钥类型

| 密钥                        | 存储位置                        | 用途                                            |
| --------------------------- | ------------------------------- | ----------------------------------------------- |
| gateway_key                 | settings.json                   | Agent 访问 Gateway 的 Bearer token              |
| Provider api_keys           | settings.providers[id].api_keys | 转发上游；按配置顺序尝试，跳过 429 冷却中的 Key |
| artificial_analysis_api_key | settings（可选）                | 拉取 AA 基准                                    |

## 端口

| 端口                  | 协议     | 来源          | 默认值 |
| --------------------- | -------- | ------------- | ------ |
| settings.gateway_port | HTTP TCP | settings.json | 3125   |

> 端口的种子值来自 `cab.toml [gateway] port`，首次安装后由 `settings.json` 管理运行时值。详见上方「端口优先级链」。

## 安全实践

- settings.json 权限依赖 OS 用户目录隔离
- 更新 Provider 时 `save_to_disk` 立即落盘
- 429 后 `quota_reset_at` 写入 settings，避免重复撞限额

## 环境变量

| 变量                   | 作用           |
| ---------------------- | -------------- |
| `HOME` / `USERPROFILE` | 解析 `~/.cab`  |
| `RUST_LOG`             | tracing 过滤器 |
