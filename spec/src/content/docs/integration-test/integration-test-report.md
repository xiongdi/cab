---
title: 集成测试报告
description: CAB 集成测试结论与系统测试准入
chapter: integration-test
order: 5
---

## 结论

| 项 | 结果 |
| --- | --- |
| 自动化集成测试 | `cargo test --workspace` 通过 |
| 接口契约 | Gateway/API/DB 边界有对应用例 |
| 跨场景 | INT-SC-01～07 可手工验证 |

## 风险项

| 风险 | 等级 | 缓解 |
| --- | --- | --- |
| models.dev 离线 | Medium | 使用 catalog 缓存 |
| 真实上游 Key 费用 | Low | 集成测用 mock/无效 Key 测路径 |
| 本地端口冲突 | Low | 调整 `settings.gateway_port` |

## 未覆盖项

- 全量多供应商真实 Key 压测
- 各 Agent 全量端到端（依赖用户本机 Agent 安装）

## 系统测试准入评估

**建议准入**，当：

1. CI main 绿
2. 单元测试报告无 Blocker
3. INT-SC-02、04、07 至少各执行一次并记录

## 交付物

- 本报告
- CI 运行链接（GitHub Actions）
- 可选：手工联调截图或 curl 日志
