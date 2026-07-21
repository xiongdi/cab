---
title: 审批记录
description: 各阶段评审、需求基线与签发留痕
chapter: appendix
order: 3
---

## 需求基线（维基未单列，记入审批）

| 类型     | 位置                                      |
| -------- | ----------------------------------------- |
| 功能需求 | `requirements/functional-requirements`    |
| URD      | `requirements/user-requirements-document` |
| UAT 计划 | `requirements/uat-plan`                   |
| 变更     | `appendix/changelog`                      |

评审检查：每条 REQ 可测试；Gateway/API 路由表无遗漏；`settings` JSON 字段变更保持 `serde(default)` 兼容；密钥仅存本地 SQLite。

## 审批流程

按 V 模型自顶向下签发基线，变更须重新评审对应阶段。

## 阶段审批表

| 阶段     | 交付物                   | 评审焦点                | 状态   | 签发人 | 日期 |
| -------- | ------------------------ | ----------------------- | ------ | ------ | ---- |
| 需求分析 | requirements/\*          | URD、UAT 计划可追溯     | 待签发 |        |      |
| 系统设计 | system-design/\*         | 与需求一致、ST 计划完整 | 待签发 |        |      |
| 架构设计 | architecture/\*          | 子系统边界、接口契约    | 待签发 |        |      |
| 模组设计 | modules/\*               | 可编码、UT 计划覆盖     | 待签发 |        |      |
| 代码实现 | 源码 + implementation/\* | 对照低阶设计            | 持续   |        |      |
| 单元测试 | unit-test/\*             | workspace 测试绿        | 持续   |        |      |
| 集成测试 | integration-test/\*      | CI 通过                 | 持续   |        |      |
| 系统测试 | system-test/\*           | ST 用例执行             | 待签发 |        |      |
| 验收测试 | acceptance/\*            | UAT Pass                | 待签发 |        |      |

## 评审检查项（通用）

- [ ] 与上游文档无矛盾
- [ ] 需求 ID / 测试 ID 可追溯
- [ ] 变更有 changelog 记录
- [ ] 源码路径与行为描述一致

## 签发说明

本仓库 spec 由开发过程同步维护；正式项目可在发布时由 PO/SA/QA 在本表填写姓名与日期。

## 关联

- 验收签字：`acceptance/acceptance-criteria.md`
- 变更历史：`appendix/changelog.md`
