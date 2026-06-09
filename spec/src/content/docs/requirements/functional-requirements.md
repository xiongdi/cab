---
title: 功能需求整理
description: CAB 可验证功能点清单
chapter: requirements
order: 3
---

## 管理 API 功能（`cab-api/src/lib.rs`）

| ID   | 功能                | 方法                | 路径                           |
| ---- | ------------------- | ------------------- | ------------------------------ |
| F-01 | 列出 LLM 提供商     | GET                 | `/api/providers`               |
| F-02 | 更新提供商 Key/端点 | PUT                 | `/api/providers/{id}`          |
| F-03 | 同步 models.dev     | POST                | `/api/providers/sync`          |
| F-04 | 列出模型            | GET                 | `/api/models`                  |
| F-05 | 三源模型目录        | GET                 | `/api/models/catalog`          |
| F-06 | 启用/禁用模型       | PUT                 | `/api/models/{id}`             |
| F-07 | 模型端点开关        | PUT                 | `/api/model-endpoints`         |
| F-08 | 路由 CRUD           | GET/POST/PUT/DELETE | `/api/routes`                  |
| F-09 | 日志查询            | GET                 | `/api/logs`                    |
| F-10 | Agent 配置          | GET/PUT             | `/api/agents/{id}`             |
| F-12 | Dashboard 统计      | GET                 | `/api/dashboard/stats`         |
| F-13 | 设置读写            | GET/PUT             | `/api/settings`                |
| F-14 | 目录同步状态        | GET                 | `/api/settings/catalog-status` |
| F-15 | 手动同步目录        | POST                | `/api/settings/sync-catalog`   |

## Gateway 功能

| ID   | 功能            | 验收条件                                           |
| ---- | --------------- | -------------------------------------------------- |
| G-01 | 请求体模型改写  | 转发上游时 JSON `model` 字段替换为目标模型名       |
| G-02 | 协议转换        | OpenAI↔Anthropic↔Responses 双向 shim               |
| G-03 | 多 Key 重试     | 429 时轮换 Key，耗尽后 fallback 下一模型           |
| G-04 | 流式 token 统计 | SSE 流经 `TokenTrackingStream` 写入日志            |
| G-05 | 模型列表伪装    | `GET /v1/models` 返回已启用模型（含 CAB 路由别名） |

## 路由解析优先级（`resolve_route`）

1. Agent `auto` 模式 + 配置的 `model_id`（路由策略 ID）
2. `find_for_agent` 匹配的首条 Route
3. 请求体 `model` 为内置策略名
4. 按名称查找具体模型
5. 否则 `NotFound`
