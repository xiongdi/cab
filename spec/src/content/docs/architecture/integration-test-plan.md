---
title: 集成测试计划
description: 架构阶段制定的 CAB 集成测试计划
chapter: architecture
order: 4
---

## 范围

验证 **crate 间接口** 与 **HTTP 路由合并** 正确，不覆盖完整 UAT 场景（见系统测试/验收章）。

## 环境与工具

- Rust `cargo test --workspace`
- 无需外部 LLM Key 的用例使用 mock / 内存 store
- 需网络的用例：models.dev catalog 解析（可标记 `#[ignore]` 或离线 fixture）

## 集成测试用例

| IT ID | 接口 | 验证点 | 源码位置 |
| --- | --- | --- | --- |
| IT-01 | `rank_models` + `effective_routing_cost` | 订阅供应商成本趋近 epsilon | `routing.rs` tests |
| IT-02 | `ordered_api_keys` | 订阅 Key 优先、429 后跳过 | `types.rs` + `subscription_quota.rs` |
| IT-03 | `resolve_route` | agent 匹配 route、auto 策略选模 | `router.rs` tests |
| IT-04 | `pick_endpoints_for_protocol` | 原生协议优先、priority 排序 | `router.rs` tests |
| IT-05 | `protocol` 转换 | OpenAI ↔ Anthropic 字段映射 | `protocol.rs` tests |
| IT-06 | `sync_models_dev_catalog` | 解析 provider JSON | `providers.rs` tests |
| IT-07 | `catalog_provider_urls` | URL 规范化 | `catalog_provider_urls.rs` tests |
| IT-08 | `benchmark_catalog` | AA 分数合并 | `benchmark_catalog.rs` tests |
| IT-09 | `apply_agent_config` | 已支持 Agent 配置写入 | `agents.rs`（手动/集成） |

## 通过准则

- Workspace `cargo test` 全绿
- 无新增 clippy `-D warnings` 违规
- 集成缺陷修复后须补充回归测试

## 准入（系统测试）

- IT-01～IT-08 自动化通过
- IT-09 或等价手工联调记录可用
