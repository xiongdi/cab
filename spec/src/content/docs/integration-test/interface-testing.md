---
title: 接口联调验证
description: 验证 CAB 子系统间接口契约
chapter: integration-test
order: 2
---

## Gateway ↔ cab-db

| 接口 | 验证 |
| --- | --- |
| `resolve_route(&InMemoryStore, ...)` | router 测试构造 store 后解析成功 |
| `cab_db::log::append` | 代理成功后日志条数增加 |
| `mark_api_key_quota_reset` | 429 后 settings.providers 含 quota_reset_at |

## cab-api ↔ cab-db

| HTTP | DB 操作 | 验证 |
| --- | --- | --- |
| PUT `/api/providers/{id}` | `provider::update` + settings 覆盖 | 响应体与 GET 一致 |
| PUT `/api/settings` | `settings::update` + 落盘 | 重启后端口保持 |
| GET `/api/models/catalog` | models + settings 合并 | 三源字段齐全 |

## cab-api ↔ 文件系统

| 接口 | 验证 |
| --- | --- |
| PUT `/api/agents/claude-code` | `~/.claude/settings.json` 含 CAB URL |

## Gateway ↔ 上游（联调）

使用无效 Key 预期 401；使用 mock server（可选）验证：

- `Authorization` 头透传
- 路径后缀 `v1/messages` / `chat/completions` 正确拼接

## 前端 ↔ cab-api

| 页面 | API | 验证 |
| --- | --- | --- |
| Providers | GET/PUT providers | 订阅开关保存后刷新仍显示 |
| Settings | PUT settings | gateway_port 变更后 Gateway 监听新端口（需重启） |
| Dashboard | GET stats | 与 logs 计数一致 |

## 错误契约

- 未知 provider id → 404 JSON
- 错误 gateway_key → Gateway 401/403（依 handler）
