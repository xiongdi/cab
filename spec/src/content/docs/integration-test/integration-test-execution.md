---
title: 集成测试计划执行
description: 执行架构设计阶段集成测试计划
chapter: integration-test
order: 1
---

## 执行方式

集成测试 = **workspace 自动化测试** + **本地多模组联调**。

```bash
cargo test -p cab-api --test agents_it
cargo test --workspace
```

架构计划 `architecture/integration-test-plan.md` 中 IT-01～IT-08 由 workspace 测试覆盖；**v0.1 Agent API** 由 `crates/cab-api/tests/agents_it.rs` 覆盖（7 智能体、移除 proxy 端点、legacy mode 归一化）。

## 执行清单

| IT ID | 状态 | 证据 |
| --- | --- | --- |
| IT-01 订阅路由成本 | 自动 | `routing.rs` tests |
| IT-02 Key 顺序与限额 | 自动 | `subscription_quota.rs` + types |
| IT-03 resolve_route | 自动 | `router.rs` tests |
| IT-04 端点选择 | 自动 | `router.rs` tests |
| IT-05 协议转换 | 自动 | `protocol.rs` tests |
| IT-06 catalog 解析 | 自动 | `providers.rs` tests |
| IT-07 URL 规范化 | 自动 | `catalog_provider_urls.rs` tests |
| IT-08 AA 合并 | 自动 | `benchmark_catalog.rs` tests |
| IT-09 Agent 配置写盘 | 手工/可选 | 更新 agent 后检查 `~/.claude/settings.json` |

## 手工联调（推荐）

1. `cargo run -p cab-server`
2. `curl http://127.0.0.1:3125/api/providers` → JSON 列表
3. `curl -H "Authorization: Bearer {gateway_key}" http://127.0.0.1:3125/v1/models` → 模型列表

## 准入系统测试

- CI 绿
- IT-09 或等价记录完成
