---
title: 持久化策略
description: CAB 配置文件、状态与日志落盘策略
chapter: system-design
order: 5
---

## 三文件策略

| 类型     | 路径                   | 内容                                      |
| -------- | ---------------------- | ----------------------------------------- |
| 用户设置 | `~/.cab/settings.json` | 端口、gateway_key、auth_enabled、Key 覆盖 |
| 业务配置 | `~/.cab/state.json`    | agents、routes                            |
| 观测数据 | `~/.cab/logs/*.jsonl`  | 请求日志（按日滚动）                      |
| 目录缓存 | `~/.cab/catalog/`      | models.dev、AA 快照（可重建）             |

## Configuration vs Runtime

| 分类          | 实体                        | 持久化        |
| ------------- | --------------------------- | ------------- |
| Configuration | Settings、Agent、Route      | 是            |
| Catalog       | Provider、Model（目录部分） | 缓存          |
| User override | ProviderUserSettings 等     | settings.json |
| Runtime       | quota_reset_at（Key 限流）  | settings.json |
| Observability | RequestLog                  | JSONL         |

## state.json 格式

```json
{
  "version": 1,
  "agents": {
    "codex": {
      "id": "codex",
      "name": "Codex",
      "mode": "auto",
      "model_id": "auto",
      "api_key": "",
      "endpoint": "",
      "updated_at": "2026-06-10T12:00:00Z"
    }
  },
  "routes": {}
}
```

## 原子写流程

```
write state.json.tmp
fsync
rename state.json.tmp → state.json
```

## 日志 JSONL

- 文件名：`requests-YYYY-MM-DD.jsonl`
- 每行一条 `RequestLog` JSON
- 启动时 `enforce_retention(log_retention_days)` 删除过期文件
- 内存保留最近 500 条加速 Dashboard

## 迁移（v0.1.x → v0.2.0）

1. 升级后首次启动：从内存默认 agents 写出 `state.json`
2. 旧 `settings.json` 无 `auth_enabled` 字段 → 反序列化默认 `true`
3. 旧硬编码 `gateway_key` 保留；新安装随机生成
