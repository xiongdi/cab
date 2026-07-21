---
title: 非功能验证
description: CAB 性能、安全、可用性与兼容性
chapter: system-test
order: 3
---

## 性能

| 项           | 目标                             | 验证方法                  |
| ------------ | -------------------------------- | ------------------------- |
| 本地路由开销 | resolve_route < 50ms（百级模型） | 日志 latency 减去上游时间 |
| 内存占用     | 单进程可接受桌面使用             | 任务管理器观察            |
| 启动时间     | catalog 同步不阻塞 HTTP 监听     | 启动后即可 curl /api      |

## 安全

| 项           | 验证                                |
| ------------ | ----------------------------------- |
| Gateway 认证 | 无 gateway_key 拒绝                 |
| Key 存储     | 仅 ~/.cab/cab.db，不入库 git        |
| 本地绑定     | 默认 127.0.0.1，不对外网暴露        |

## 可用性

| 项            | 验证                            |
| ------------- | ------------------------------- |
| 离线 settings | 损坏 JSON 回退 default_settings |
| 上游失败      | 明确错误 JSON，不 panic         |
| Fallback      | 主路径失败有备选                |

## 兼容性

| 客户端        | 协议            | 端口                            |
| ------------- | --------------- | ------------------------------- |
| OpenAI SDK    | chat/responses  | gateway_port（默认 3125，HTTP） |
| Anthropic SDK | messages        | 同上                            |
| Claude Code   | settings 重定向 | 同上                            |

## 协议与传输

- HTTP/1.1（axum）

## 日志保留

`log_retention_days` 默认 30；超期清理逻辑在 log 模组（若实现）或文档约定。
