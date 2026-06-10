---
title: 系统架构与界面结构
description: CAB 逻辑分层与管理界面菜单
chapter: system-design
order: 2
---

## 逻辑分层

| 层     | 组件                  | 职责                         |
| ------ | --------------------- | ---------------------------- |
| 展示层 | `src/routes/*.svelte` | 配置、监控、i18n、路由预览   |
| API 层 | `cab-api`             | REST 薄 adapter              |
| 网关层 | `cab-gateway`         | 鉴权、协议 adapter、代理执行 |
| 应用层 | `cab-services`        | 目录同步、路由、Agent 编排   |
| 领域层 | `cab-core`            | 路由算法、类型               |
| 持久层 | `cab-db`              | Store、settings/state/logs   |

## 界面菜单结构（`Sidebar.svelte` + `translations.ts`）

| 导航项    | 路由         | 功能                               |
| --------- | ------------ | ---------------------------------- |
| Dashboard | `/`          | 请求量、token、提供商/模型分布     |
| Providers | `/providers` | Key、端点、订阅标记、启用          |
| Models    | `/models`    | 三源目录、端点定价、启用           |
| Routes    | `/routes`    | 内置策略说明、路由预览（模拟请求） |
| Agents    | `/agents`    | 7 种 Agent 模式配置                |
| Logs      | `/logs`      | 请求日志筛选分页                   |
| Settings  | `/settings`  | 网关端口、密钥、目录同步           |

## 桌面壳（Tauri）

- `src-tauri/src/lib.rs` 后台启动 Axum，窗口导航至 `http://127.0.0.1:{port}`
- 命令 `get_gateway_port` 供前端解析 API 基址（`api.ts`）

## 网关对外界面

与 OpenAI/Anthropic SDK 兼容的 HTTP 路径，见 `gateway/server.rs` 路由表。
