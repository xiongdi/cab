---
title: 投产与移交
description: CAB 上线步骤与交付物
chapter: acceptance
order: 5
---

## 投产步骤

1. **构建**：`cargo build --release -p cab-server` 或 `npm run tauri:build`
2. **前端**：`npm run build` 将 `build/` 置于 server 工作目录
3. **配置**：首次启动生成 `~/.cab/settings.json`
4. **同步**：Settings 页触发 catalog 同步
5. **LLM 提供商**：在 Providers 页录入 API Key，标记订阅套餐
6. **Agent**：Agents 页设 native/auto/manual，验证配置文件
7. **验证**：执行 UAT-01 冒烟

## 运维移交

| 项   | 说明                                                         |
| ---- | ------------------------------------------------------------ |
| 进程 | 单进程 Axum，无独立 DB 服务                                  |
| 日志 | `RUST_LOG=info`；请求 tracing                                |
| 备份 | 定期备份 `~/.cab/settings.json`                              |
| 升级 | 拉取新版本 → 重建 → 重启；settings 向前兼容（serde default） |

## 交付物清单

| 交付物     | 位置                             |
| ---------- | -------------------------------- |
| 可执行文件 | `cab-server` / Tauri 安装包      |
| 管理 UI    | `build/` 或内嵌                  |
| 规格文档站 | `spec/` Astro 构建产物           |
| 默认配置   | `config/*.json`、`cab.toml` 示例 |
| CI 定义    | `.github/workflows/ci.yml`       |

## 用户文档入口

- 本地 UI：各页内嵌说明与 `Routes` 策略解释
- 规格站：`spec/` → `npm run dev` 或静态托管 `dist/`

## 支持边界

CAB 为**本地**网关，不提供云端 SLA；上游 LLM 提供商故障不在 CAB 支持范围内。
