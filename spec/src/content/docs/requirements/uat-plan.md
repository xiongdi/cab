---
title: 用户验收测试计划
description: CAB UAT 计划，需求阶段制定
chapter: requirements
order: 4
---

## UAT 范围

在**用户本机环境**验证 CAB 能否让编程智能体无感接入多模型路由。

### 环境要求

- OS：Linux / macOS / Windows（Tauri 构建矩阵见 `build-tauri.yml`）
- Rust 2024 + Node 18+
- 至少一个已配置提供商 Key

## UAT 用例（映射 REQ）

| UAT ID | 需求        | 步骤                                            | 通过准则                                 |
| ------ | ----------- | ----------------------------------------------- | ---------------------------------------- |
| UAT-01 | REQ-CAB-001 | Claude Code 配置 CAB 网关，发起 coding 请求     | 收到有效补全，日志有记录                 |
| UAT-02 | REQ-CAB-002 | Agent 设 `auto`，发送简单 vs 复杂 prompt        | 简单倾向便宜模型，复杂倾向高能力模型     |
| UAT-03 | REQ-CAB-003 | UI 配置 Anthropic Key 并启用                    | Dashboard 显示 active_providers ≥ 1      |
| UAT-04 | REQ-CAB-004 | 标记订阅 Key，触发 429（或模拟）                | 自动 fallback 至按量 Key 或其他模型      |
| UAT-05 | REQ-CAB-005 | 完成 10 次请求                                  | 日志页可筛选、分页正确                   |
| UAT-06 | REQ-CAB-006 | 切换 claude-code 为 `auto` 模式                 | `~/.claude/settings.json` 写入 CAB 端点  |
| UAT-07 | —           | `npm run tauri:dev` 启动桌面端                  | 七页面可导航、i18n 切换正常              |
| UAT-08 | —           | `POST /api/settings/sync-catalog`               | 返回 `success: true` 且模型数 > 0        |
| UAT-09 | REQ-CAB-007 | 修改 Agent 模式后重启 cab-server                | `~/.cab/state.json` 保留配置             |
| UAT-10 | REQ-CAB-008 | 无 Authorization 访问 `/v1/models`              | HTTP 401；带正确 Bearer 返回 200         |
| UAT-11 | REQ-CAB-009 | 完成请求后重启，查询 `/api/logs`                | 历史日志仍可分页查询                     |
| UAT-12 | REQ-CAB-010 | Routes 页模拟请求 + `POST /api/routing/explain` | 返回 decision_steps 与 ranked_candidates |

## 数据策略

使用用户自有 API Key；不可用生产密钥写入 spec 或日志导出。

## 签字

| 角色     | 姓名 | 日期 | 结论 |
| -------- | ---- | ---- | ---- |
| 用户代表 |      |      |      |
| 产品     |      |      |      |
