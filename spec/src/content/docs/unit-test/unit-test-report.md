---
title: 单元测试报告
description: CAB 单元测试结论与下游输入
chapter: unit-test
order: 5
---

## 摘要

| 项 | 值 |
| --- | --- |
| 执行方式 | `cargo test --workspace` |
| 自动化 | 是（CI rust-checks job） |
| 主要 crate | cab-core, cab-gateway, cab-api |
| 当前状态 | 与 main 分支 CI 一致即为通过 |

## 覆盖率说明

CAB 未强制行覆盖率工具；以 **行为覆盖** 为准：

- 四种路由策略均有断言
- 订阅 Key 成本与排序有断言
- 429 quota_reset_at 解析有断言
- 协议转换有 fixture 测试

## 模组通过率

| Crate | 测试模块数 | 备注 |
| --- | --- | --- |
| cab-core | 5+ | routing、quota、benchmark 等 |
| cab-gateway | 2+ | router、protocol |
| cab-api | 3+ | providers、urls、hook |
| cab-db | 随集成验证 | 以 store 操作为主 |

## 遗留缺陷

无 Blocker 级单元缺陷进入集成阶段。新功能合并前须保持 workspace 测试绿。

## 对集成测试的输入

- 路由算法已验证 → IT 聚焦 `resolve_route` + store
- Key 顺序已验证 → IT 聚焦 `execute_with_fallback` 与持久化
- 协议转换已验证 → IT 聚焦端到端 handler（可选 mock 上游）

## 签发

单元测试通过为 **集成测试准入** 必要条件之一。
