---
title: V 模型对应关系表
description: 维基百科 V 模型各阶段与 CAB spec 小节映射
chapter: appendix
order: 1
---

依据：[V 模型（软件开发）](https://zh.wikipedia.org/wiki/V%E6%A8%A1%E5%9E%8B_(%E8%BB%9F%E9%AB%94%E9%96%8B%E7%99%BC)

## 左右臂一一对应

| 左臂（项目定义） | 小节数 | 右臂（确认） | 小节数 |
| --- | --- | --- | --- |
| 需求分析 | 4 | 用户验收测试 | 5 |
| 系统设计 | 5 | 系统测试 | 4 |
| 架构设计 | 4 | 集成测试 | 3 |
| 模组设计 | 5 | 单元测试 | 3 |
| 代码实现 | 4 | — | — |

## 需求分析 → 用户验收测试

| 需求分析交付物 | spec 路径 | → 验收验证 |
| --- | --- | --- |
| 用户需要分析 | `requirements/user-needs` | `acceptance/uat-execution` |
| 用户需求文件 URD | `requirements/user-requirements-document` | `acceptance/acceptance-criteria` |
| 功能需求 | `requirements/functional-requirements` | `acceptance/acceptance-criteria` |
| UAT 计划（早期制定） | `requirements/uat-plan` | `acceptance/uat-execution` |
| — | — | `acceptance/user-environment` |
| — | — | `acceptance/real-data-validation` |
| — | — | `acceptance/release-handover` |

## 系统设计 → 系统测试

| 系统设计交付物 | spec 路径 | → 系统测试 |
| --- | --- | --- |
| 软件规格说明书 | `system-design/software-specification` | `system-test/system-test-execution` |
| 架构与界面 | `system-design/architecture-and-ui` | `system-test/functional-verification` |
| 业务场景 | `system-design/business-scenarios` | `system-test/functional-verification` |
| 实体与数据字典 | `system-design/entity-data-dictionary` | `system-test/functional-verification` |
| 系统测试计划（早期制定） | `system-design/system-test-plan` | `system-test/system-test-execution` |
| — | — | `system-test/nonfunctional-verification` |
| — | — | `system-test/system-test-report` |

## 架构设计 → 集成测试

| 架构设计交付物 | spec 路径 | → 集成测试 |
| --- | --- | --- |
| 子系统划分 | `architecture/subsystem-decomposition` | `integration-test/integration-test-execution` |
| 集成策略 | `architecture/integration-strategy` | `integration-test/integration-test-execution` |
| 接口契约 | `architecture/interface-contracts` | `integration-test/interface-testing` |
| 集成测试计划（早期制定） | `architecture/integration-test-plan` | `integration-test/integration-test-execution` |
| — | — | `integration-test/integration-test-report` |

## 模组设计 → 单元测试

| 模组设计交付物 | spec 路径 | → 单元测试 |
| --- | --- | --- |
| 模组拆解 | `modules/module-decomposition` | `unit-test/unit-test-execution` |
| 逻辑与伪代码 | `modules/logic-pseudocode` | `unit-test/isolation-testing` |
| 数据库表 | `modules/database-tables` | `unit-test/isolation-testing` |
| 接口与 I/O | `modules/api-dependencies-io` | `unit-test/isolation-testing` |
| 单元测试计划（早期制定） | `modules/unit-test-plan` | `unit-test/unit-test-execution` |
| — | — | `unit-test/unit-test-report` |

## CAB 需求追溯（REQ-CAB）

| REQ ID | 定义阶段 | 确认阶段 |
| --- | --- | --- |
| REQ-CAB-001 | `requirements/functional-requirements` | UAT-01 |
| REQ-CAB-002 | `system-design/software-specification` | ST-05 / UAT-02 |
| REQ-CAB-003 | `requirements/functional-requirements` | UAT-03 |
| REQ-CAB-004 | `modules/module-decomposition`（agents） | UAT-06 |
| REQ-CAB-005 | `system-design/architecture-and-ui` | UAT-04 |
| REQ-CAB-006 | `modules/logic-pseudocode` | UAT-07/08 |

需求基线评审记入 `appendix/approval.md`，不单独占需求分析一章一节（维基条目未单列）。
