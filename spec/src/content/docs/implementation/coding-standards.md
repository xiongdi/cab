---
title: 编码规范
description: CAB Rust 与 TypeScript 编码约定
chapter: implementation
order: 2
---

## Rust

| 项       | 约定                                                     |
| -------- | -------------------------------------------------------- |
| 格式化   | `cargo fmt --all`（CI 强制 `--check`）                   |
| Lint     | `cargo clippy --workspace --all-targets -- -D warnings`  |
| 错误类型 | 库层 `CabError`；handler 层 `IntoResponse`               |
| 异步     | `tokio`；DB 操作为 `async fn` 但内部 `RwLock` 同步       |
| 日志     | `tracing`；网关 `cab_gateway=debug`、API `cab_api=debug` |
| 序列化   | `serde`；字段默认值用 `#[serde(default)]`                |
| 测试     | 同文件 `#[cfg(test)] mod tests`                          |

## TypeScript / Svelte

| 项   | 约定                                  |
| ---- | ------------------------------------- |
| 检查 | `npm run check`（svelte-check）       |
| 类型 | `src/lib/types.ts` 与 Rust 结构对齐   |
| API  | 统一经 `src/lib/api.ts`，不散落 fetch |
| i18n | `translations.ts` 中英文键值          |
| 组件 | Svelte 5 runes；页面在 `src/routes/`  |

## 命名

- Rust：snake_case 函数/模块，PascalCase 类型
- API 路径：kebab 资源名，`{id}` 路径参数
- Agent id：小写连字符（`claude-code`）

## 安全

- API Key、gateway_key 仅存本地 `~/.cab/cab.db` 与内存
- 不向日志打印完整 Key（仅 debug 级别谨慎输出）

## 可测试性

- 纯函数置于 `cab-core`，避免 Axum 依赖
- 路由解析接受 `&InMemoryStore`，测试可构造内存数据
- 协议转换独立于 HTTP 客户端
