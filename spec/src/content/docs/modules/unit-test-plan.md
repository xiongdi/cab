---
title: 单元测试计划
description: 模组设计阶段制定的 CAB 单元测试计划
chapter: modules
order: 5
---

## 目标

在 **隔离环境** 验证各 crate 最小逻辑单元，消除算法与解析类缺陷。

## 测试分布（源码内 `#[cfg(test)]`）

| Crate       | 模块                       | 用例主题                                                     |
| ----------- | -------------------------- | ------------------------------------------------------------ |
| cab-core    | `routing.rs`               | effective_token_cost、订阅成本、rank_models 排序、复杂度门槛 |
| cab-core    | `subscription_quota.rs`    | Retry-After 秒/HTTP-date、is_key_rate_limited                |
| cab-core    | `provider_defaults.rs`     | 默认端点注入                                                 |
| cab-core    | `benchmark_catalog.rs`     | AA JSON 解析、分数合并                                       |
| cab-core    | `model_scores.rs`          | 启发式分数                                                   |
| cab-gateway | `router.rs`                | pick_endpoints、resolve 分支                                 |
| cab-gateway | `protocol.rs`              | OpenAI↔Anthropic 字段                                        |
| cab-api     | `providers.rs`             | catalog 解析                                                 |
| cab-api     | `catalog_provider_urls.rs` | URL 规范化                                                   |

## 单元测试用例（UT 抽样）

| UT ID | 模块                          | 断言                                     |
| ----- | ----------------------------- | ---------------------------------------- |
| UT-01 | `effective_routing_cost`      | 订阅 provider_id 在集合内 → cost ≈ 0.001 |
| UT-02 | `ordered_api_keys`            | 订阅 Key 排在按量 Key 之前               |
| UT-03 | `is_key_rate_limited`         | quota_reset_at 未来 → true               |
| UT-04 | `RoutingStrategy::parse`      | `"price"` → Cheapest                     |
| UT-05 | `build_request_profile`       | 含 tool 调用 → Agentic                   |
| UT-06 | `pick_endpoints_for_protocol` | 同协议 priority 高者在前                 |
| UT-07 | `protocol` 转换               | messages 格式互转字段不丢失              |
| UT-08 | `resolve_quota_reset_at`      | Header `Retry-After: 120` → 120s         |

## 执行方式

```bash
cargo test -p cab-core
cargo test -p cab-gateway
cargo test -p cab-api
cargo test -p cab-db
```

## 通过准则

- 各 crate 测试 100% 通过
- 新增业务逻辑须同 PR 附带测试
- 覆盖率不设硬性百分比；重点覆盖路由、订阅、协议转换

## 出口（集成测试准入）

单元测试报告无 Blocker；遗留项记录于 `unit-test/unit-test-report.md`。
