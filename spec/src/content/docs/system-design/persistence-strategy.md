---
title: 持久化策略
description: CAB 配置、状态与日志落盘策略（以 SQLite 为准）
chapter: system-design
order: 5
---

## 单一数据库策略

运行时唯一配置/状态源：`~/.cab/cab.db`（SQLite，`SCHEMA_VERSION = 4`）。权威 DDL 见 `crates/cab-db/src/sqlite.rs`。

| 类型     | 存储位置                         | 内容                                                         |
| -------- | -------------------------------- | ------------------------------------------------------------ |
| 用户设置 | `settings` 表（`id=1` JSON）     | 端口、gateway_key、auth_enabled、providers/models 覆盖等     |
| 业务配置 | `agents` / `routes` 表           | Agent 模式、路由规则                                         |
| 观测数据 | `request_logs` / `usage_records` | 请求日志与用量；保留天数 `log_retention_days`（默认 30）     |
| 目录数据 | `catalog_*` / `model_endpoints` / `aa_benchmark_records` | models.dev 与 AA 同步结果                   |
| 目录缓存 | `~/.cab/catalog/`                | models.dev 等下载缓存（可重建）                              |
| 系统引导 | `cab.toml`                       | `gateway.host`；`gateway.port` 仅作首次安装种子              |

已废弃（勿作运行时配置）：`~/.cab/settings.json`、`~/.cab/state.json`、`~/.cab/logs/*.jsonl`。

## Configuration vs Runtime

| 分类          | 实体                        | 持久化                         |
| ------------- | --------------------------- | ------------------------------ |
| Configuration | Settings、Agent、Route      | SQLite                         |
| Catalog       | Provider、Model（目录部分） | SQLite + `~/.cab/catalog/` 缓存 |
| User override | Provider/Model 覆盖         | `settings.data` JSON           |
| Runtime       | quota / subscription        | `subscription_quotas` 等       |
| Observability | RequestLog、Usage           | SQLite 表                      |

## 写路径（概要）

1. API / gateway 更新内存 `InMemoryStore`
2. 对应模块调用 `cab-db` 写入 SQLite（settings / agents / routes / logs）
3. 日志查询经 `GET /api/logs`，受 `log_retention_days` 清理

## 端口优先级

SQLite `settings.gateway_port`（运行时）→ `cab.toml [gateway] port`（引导 fallback）→ `3125`。Host 始终来自 `cab.toml`。

## 历史迁移说明

旧版曾使用 `settings.json` / `state.json` / JSONL；现已迁移至 SQLite。恢复脚本 `scripts/restore-keys-from-backup.py` 仅用于从旧备份读回 key，不是运行时路径。
