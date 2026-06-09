---
title: 单元测试计划执行
description: 执行模组设计阶段单元测试计划
chapter: unit-test
order: 1
---

## 执行记录

| 日期 | 命令                     | 结果            |
| ---- | ------------------------ | --------------- |
| 持续 | `cargo test --workspace` | CI 每次 PR 执行 |

本地执行：

```bash
cargo test -p cab-core -p cab-gateway -p cab-api -p cab-db
```

## 覆盖模组

对照 `modules/unit-test-plan.md`：

- **cab-core**：routing、subscription_quota、benchmark_catalog、model_scores、provider_defaults
- **cab-gateway**：router、protocol
- **cab-api**：providers、catalog_provider_urls、`agents::normalize_agent_mode`
- **cab-db**：`agent` 默认 7 智能体、移除 cursor/antigravity、legacy `proxy`→`native`

## 执行环境

- Rust stable（与 CI `dtolnay/rust-toolchain@stable` 一致）
- 无外部网络依赖（单元测试使用内联 JSON fixture）

## 结论

Workspace 内 `#[test]` 用例作为单元测试计划的主要执行载体；新增功能须在对应模组补充测试后合并。
