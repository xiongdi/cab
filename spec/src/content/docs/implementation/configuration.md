---
title: 配置与密钥管理
description: CAB 配置文件、路径与密钥存储
chapter: implementation
order: 4
---

## 配置文件

| 文件                                      | 加载方              | 角色                                               | 内容                                                                                                                 |
| ----------------------------------------- | ------------------- | -------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `cab.toml`                                | `CabConfig::load()` | **系统引导**（启动时读一次，不可通过 API 修改）    | `gateway.host`（默认 127.0.0.1）、`gateway.port`（首次安装写入数据库的种子值）                                       |
| `~/.cab/cab.db`                           | `cab-db`            | **用户运行时**（通过 SQLite 数据库 `settings` 表） | `gateway_port`（运行时端口）、`gateway_key`、`auth_enabled`、`log_retention_days`、`providers`、`models`、`api_keys` |
| `config/provider-endpoints.defaults.json` | `provider_defaults` | 静态默认                                           | 提供商默认端点                                                                                                       |
| `config/aa-model-map.json`                | `benchmark_catalog` | 静态默认                                           | AA 模型名映射                                                                                                        |

### 端口优先级链

SQLite `settings` 里的 `gateway_port`（运行时，API 可编辑）→ `cab.toml [gateway] port`（引导默认）→ 硬编码 `3125`。

- 首次安装：`cab.toml` 的 port 写入数据库，之后运行时始终读数据库
- 用户通过 Settings 页面改端口 → 写入数据库，重启后生效
- `cab.toml` 的 port 仅在数据库加载失败时作为 fallback

### Host vs Port 分工

| 配置项         | 来源            | 可通过 API 修改 | 说明                                   |
| -------------- | --------------- | --------------- | -------------------------------------- |
| `gateway.host` | `cab.toml`      | 否              | 系统级绑定地址，始终从 `cab.toml` 读取 |
| `gateway.port` | SQLite settings | 是              | 运行时端口，首次安装从 `cab.toml` 种子 |

## 运行时路径

| 路径                         | 用途                                       |
| ---------------------------- | ------------------------------------------ |
| `~/.cab/cab.db`              | SQLite 数据库存储                          |
| `~/.cab/catalog/`            | models.dev / AA 缓存                       |
| `~/.claude/settings.json` 等 | Agent 配置（由 `apply_agent_config` 写入） |

## 密钥类型

| 密钥                        | 存储位置                       | 用途                                            |
| --------------------------- | ------------------------------ | ----------------------------------------------- |
| gateway_key                 | SQLite settings 表             | Agent 访问 Gateway 的 Bearer token              |
| Provider api_keys           | SQLite settings 里的 providers | 转发上游；按配置顺序尝试，跳过 429 冷却中的 Key |
| artificial_analysis_api_key | SQLite settings 表（可选）     | 拉取 AA 基准                                    |

## 端口

| 端口                  | 协议     | 来源            | 默认值 |
| --------------------- | -------- | --------------- | ------ |
| settings.gateway_port | HTTP TCP | SQLite settings | 3125   |

> 端口的种子值来自 `cab.toml [gateway] port`，首次安装后由 SQLite 数据库管理运行时值。详见上方「端口优先级链」。

## 安全实践

- `cab.db` 权限依赖 OS 用户目录隔离
- 更新 Provider 时直接保存至 SQLite 数据库
- 429 后 `quota_reset_at` 写入数据库，避免重复撞限额

## 环境变量

| 变量                   | 作用           |
| ---------------------- | -------------- |
| `HOME` / `USERPROFILE` | 解析 `~/.cab`  |
| `RUST_LOG`             | tracing 过滤器 |
