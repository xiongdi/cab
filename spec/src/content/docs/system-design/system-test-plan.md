---
title: 系统测试计划
description: 系统设计阶段制定的 CAB 系统测试计划
chapter: system-design
order: 5
---

## 测试范围

验证 CAB 作为**完整系统**（Gateway + API + UI + 持久化）满足 URD 全部功能需求。

## 环境

| 项 | 配置 |
| --- | --- |
| 启动方式 | `cargo run -p cab-server` 或 `npm run tauri:dev` |
| 端口 | 默认 HTTP `3125`（`settings.gateway_port`） |
| 数据 | `~/.cab/settings.json` 测试用副本 |

## 系统测试用例

| ST ID | 场景 | 步骤 | 期望 |
| --- | --- | --- | --- |
| ST-01 | 冷启动 | 删缓存后启动，触发 catalog sync | providers/models 非空 |
| ST-02 | 管理 API 全链路 | 依次调用 F-01～F-15 | 均 2xx |
| ST-03 | OpenAI 代理 | curl chat/completions | JSON 响应含 choices |
| ST-04 | Anthropic 代理 | curl messages | 响应可解析 |
| ST-05 | 策略路由 | 同 agent 不同复杂度 body | 选中模型不同 |
| ST-06 | Fallback | 主模型 Key 无效 | 尝试 fallback 模型 |
| ST-07 | 设置持久化 | 改 gateway_port 重启 | 端口保持 |
| ST-08 | Dashboard | GET stats | 数字与 logs 一致 |
| ST-09 | 前端构建 | `npm run build` | 无错误 |
| ST-10 | Workspace 测试 | `cargo test --workspace` | 全通过 |

## 非功能（见 system-test 章）

性能基线、日志保留策略单独用例。

## 入口准则

- 集成测试报告 IT 通过或遗留项已评估
- CI `main` 分支绿

## 出口准则

- 全部 ST 用例通过
- 无 Blocker 级缺陷
- 系统测试报告已签发
