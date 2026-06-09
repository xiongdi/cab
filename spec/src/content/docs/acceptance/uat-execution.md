---
title: 用户验收测试执行
description: 执行需求阶段 UAT 计划
chapter: acceptance
order: 1
---

## 依据

`requirements/uat-plan.md` 定义 UAT-01～UAT-08，追溯 REQ-CAB-001～006。

## 自动化冒烟（发布前）

```bash
./scripts/verify-v0.1.sh
```

串联 UT（cab-db/cab-api/cab-core/cab-gateway）→ IT（`agents_it`）→ ST（`system_v01`）→ 全 workspace 回归 → `npm run check`。

## 前置条件（手工 UAT）

- CAB 已安装：`cargo run -p cab-server` 或 Tauri 安装包
- 用户已配置至少一个有效 LLM 提供商 Key
- Agent（如 Claude Code / Codex）可指向 `http://127.0.0.1:{gateway_port}`

## UAT 执行表

| UAT ID | 用户故事    | 步骤                              | 通过标准               |
| ------ | ----------- | --------------------------------- | ---------------------- |
| UAT-01 | 统一接入    | 配置 Agent 使用 CAB Gateway       | Agent 可正常对话       |
| UAT-02 | 自动选模    | Agent 使用 auto，完成 coding 任务 | 选中合适模型且响应正确 |
| UAT-03 | 提供商管理  | LLM 提供商页配置 Key 并启用       | 目录模型可见且可路由   |
| UAT-04 | 成本可见    | Dashboard / Logs 查看 token       | 数字合理               |
| UAT-05 | 多协议      | 换用 Anthropic/OpenAI 客户端      | 均可通过 CAB           |
| UAT-06 | Claude Code | Agents 设 auto，启动 Claude Code  | 无感走 CAB             |
| UAT-07 | 订阅优先    | 标记订阅 Key，观察路由            | 优先使用订阅额度       |
| UAT-08 | 额度恢复    | 触发 429 后等待或 fallback        | 不中断工作或自动切换   |

## 执行角色

**企业用户**（开发者）在本机执行；测试人员记录结果与截图。

## 记录

每项标注 Pass / Fail / Blocked，Fail 附需求 ID 与现象描述。
