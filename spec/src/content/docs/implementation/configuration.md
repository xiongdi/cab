---
title: 配置与密钥管理
description: CAB 配置文件、路径与密钥存储
chapter: implementation
order: 4
---

## 配置文件

| 文件 | 加载方 | 内容 |
| --- | --- | --- |
| `cab.toml` | `CabConfig::load()` | `gateway.host`（默认 127.0.0.1）、`gateway.port`（默认 3125） |
| `~/.cab/settings.json` | `cab-db::settings` | 端口覆盖、gateway_key、供应商/模型启用、api_keys（含 subscribed、quota_reset_at） |
| `config/provider-endpoints.defaults.json` | `provider_defaults` | 供应商默认端点 |
| `config/aa-model-map.json` | `benchmark_catalog` | AA 模型名映射 |

## 运行时路径

| 路径 | 用途 |
| --- | --- |
| `~/.cab/settings.json` | 用户设置持久化 |
| `~/.cab/catalog/` | models.dev / AA 缓存 |
| `~/.claude/settings.json` 等 | Agent 配置（由 `apply_agent_config` 写入） |

## 密钥类型

| 密钥 | 存储位置 | 用途 |
| --- | --- | --- |
| gateway_key | settings.json | Agent 访问 Gateway 的 Bearer token |
| Provider api_keys | settings.providers[id].api_keys | 转发上游 |
| artificial_analysis_api_key | settings（可选） | 拉取 AA 基准 |
| subscribed 标记 | ApiKeyConfig.subscribed | 路由成本优先，非独立密钥 |

## 端口

| 端口 | 协议 | 说明 |
| --- | --- | --- |
| settings.gateway_port | HTTP TCP | 默认 3125 |

## 安全实践

- settings.json 权限依赖 OS 用户目录隔离
- 更新 Provider 时 `save_to_disk` 立即落盘
- 429 后 `quota_reset_at` 写入 settings，避免重复撞限额

## 环境变量

| 变量 | 作用 |
| --- | --- |
| `HOME` / `USERPROFILE` | 解析 `~/.cab` |
| `RUST_LOG` | tracing 过滤器 |
