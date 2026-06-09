---
title: 验收结论与签字
description: CAB 是否符合用户需求与验收标准
chapter: acceptance
order: 4
---

## 验收标准（源自 URD）

| REQ ID      | 标准                      | 验收方法       |
| ----------- | ------------------------- | -------------- |
| REQ-CAB-001 | 多 Agent 统一本地 Gateway | UAT-01、UAT-05 |
| REQ-CAB-002 | 按任务自动选模            | UAT-02         |
| REQ-CAB-003 | 提供商/模型可配置         | UAT-03         |
| REQ-CAB-004 | Agent 无感接入            | UAT-06         |
| REQ-CAB-005 | 请求可观测                | UAT-04         |
| REQ-CAB-006 | 订阅优先与 429 fallback   | UAT-07、UAT-08 |

## 通过准则

- UAT-01～08 **全部 Pass**，或 Fail 项有已批准豁免
- 系统测试无开放 Blocker/Critical
- 用户环境检查清单完成

## 验收结论模板

```
产品：CAB 本地 LLM 网关
版本：________________
日期：________________

结论：[ ] 通过  [ ] 有条件通过  [ ] 不通过

未关闭项：
- ...

签字：
  用户代表：__________
  产品负责人：__________
```

## 有条件通过

允许遗留 **Minor** 项，须附：

- 问题描述
- 规避措施
- 计划修复版本

## 追溯

本结论对应 `requirements/user-requirements-document.md` 基线；变更须走 `requirements-baseline.md` 流程。
