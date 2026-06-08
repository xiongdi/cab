---
title: 系统测试计划执行
description: 执行系统设计阶段系统测试计划
chapter: system-test
order: 1
---

## 依据

`system-design/system-test-plan.md` 定义 ST-01～ST-10。

## 自动化 ST（v0.1）

```bash
cargo test -p cab-server --test system_v01
```

覆盖：合并路由 `GET /v1/models`、`POST /v1internal:*` 不可达、`GET /api/agents` 仅 native/auto/manual。

## 执行环境（手工 ST）

```bash
npm run build
cargo run -p cab-server
# 或 npm run tauri:dev
```

数据目录：测试用 `~/.cab/settings.json` 备份后恢复。

## 执行记录模板

| ST ID | 描述 | 结果 | 备注 |
| --- | --- | --- | --- |
| ST-01 | 冷启动 catalog 同步 | Pass | 启动日志含 Synced N models |
| ST-02 | 管理 API F-01～F-15 | Pass | 见 functional-requirements |
| ST-03 | OpenAI chat | Pass/Skip | 需有效上游 Key |
| ST-04 | Anthropic messages | Pass/Skip | 需有效上游 Key |
| ST-05 | 策略路由差异 | Pass | 不同 body 选中不同模型 |
| ST-06 | Fallback | Pass/Skip | 需可控失败 Key |
| ST-07 | 设置持久化 | Pass | 改端口重启验证 |
| ST-08 | Dashboard 一致 | Pass | stats vs logs |
| ST-09 | 前端 build | Pass | CI frontend-checks |
| ST-10 | Workspace 测试 | Pass | CI rust-checks |

## 执行命令摘要

```bash
cargo test --workspace
npm run check && npm run build
curl -s http://127.0.0.1:3125/api/dashboard/stats
curl -s -H "Authorization: Bearer $GATEWAY_KEY" \
  http://127.0.0.1:3125/v1/models
```

## 出口

全部 ST 通过或 Skip 项有书面理由 → 进入验收测试评估。
