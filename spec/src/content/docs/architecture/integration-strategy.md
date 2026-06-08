---
title: 集成策略
description: CAB 持续集成与自底向上组装策略
chapter: architecture
order: 2
---

## 策略选择

CAB 采用 **自底向上 + 持续集成**：

1. **单元层**：`cab-core`、`cab-db` 纯函数与存储逻辑独立测试
2. **组件层**：`cab-gateway::router`、`protocol` 与 mock store 集成
3. **服务层**：`cab-api` handler 与真实 `InMemoryStore`
4. **系统层**：`cab-server` 全路由 + 前端 build 产物

无大爆炸发布：每次 PR 在 CI 跑完整 workspace 测试与前端 build。

## 集成顺序（与依赖方向一致）

```
cab-core
  ↓
cab-db
  ↓
cab-api ──┐
cab-gateway ──┤ 共享 InMemoryStore
  ↓
cab-server / src-tauri
  ↓
SvelteKit 静态资源（build/）
```

## CI 流水线（`.github/workflows/ci.yml`）

| Job | 步骤 |
| --- | --- |
| rust-checks | `cargo fmt --check` → `cargo clippy -D warnings` → `cargo test --workspace` |
| frontend-checks | `npm ci` → `svelte-kit sync` → `npm run check` → `npm run build` |

## 本地集成验证

```bash
cargo test --workspace
npm run build
cargo run -p cab-server   # 合并 gateway + api + 静态前端
```

## 风险与缓解

| 风险 | 缓解 |
| --- | --- |
| 内存 store 与磁盘 settings 不一致 | `update_settings` 同步写盘 |
| 协议转换错误 | `protocol.rs` 单元测试 + Gateway 集成用例 |
| 端口配置错误 | 启动日志明确监听地址 |
