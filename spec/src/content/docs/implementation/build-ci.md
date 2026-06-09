---
title: 构建与持续集成
description: CAB 本地构建与 GitHub Actions
chapter: implementation
order: 3
---

## 本地构建

### 后端

```bash
# 开发运行
cargo run -p cab-server

# 发布构建
cargo build --release -p cab-server
```

依赖：OpenSSL、系统库（Linux 上 webkit2gtk 等，见 CI apt 列表）。

### 前端

```bash
npm ci
npx svelte-kit sync
npm run build        # 输出 build/
npm run check        # 类型检查
```

### 桌面

```bash
npm run tauri:dev
npm run tauri:build
```

## Workspace 成员（`Cargo.toml`）

`cab-core`, `cab-db`, `cab-api`, `cab-gateway`, `cab-server`, `src-tauri`

## CI（`.github/workflows/ci.yml`）

触发：`push` / `pull_request` → `main`

| Job             | 命令                                                             |
| --------------- | ---------------------------------------------------------------- |
| Rust Checks     | `cargo fmt --check`                                              |
|                 | `cargo clippy --workspace --all-targets -- -D warnings`          |
|                 | `cargo test --workspace`                                         |
| Frontend Checks | `npm ci` → `svelte-kit sync` → `npm run check` → `npm run build` |

## 启动时行为

`cab-server` 启动后：

1. `init_store()` 加载 settings
2. 后台 `sync_models_dev_catalog`
3. 监听 HTTP `settings.gateway_port`（默认 3125）
4. 若存在 `build/` 则托管前端

## 配置加载

- `cab.toml`：host/port 默认（可被 settings.gateway_port 覆盖）
- 环境变量：`RUST_LOG` 控制 tracing
