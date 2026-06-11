---
title: 更新日志
description: CAB 版本发布记录。
---

CAB 遵循 [语义化版本](https://semver.org/lang/zh-CN/)。完整日志：[GitHub CHANGELOG.md](https://github.com/xiongdi/cab/blob/main/CHANGELOG.md)。

## v0.2.4

- 官方文档站 [xiongdi.github.io/cab](https://xiongdi.github.io/cab/)（中英双语，GitHub Pages 自动部署）
- 新增使用指南与参考文档：路由、Agent、提供商、网关认证、架构、API

## v0.2.3

- **Codex**：CAB 管理模式下通过 `auth.json` 动态认证（ChatGPT OAuth）
- **Codex**：切换模式时自动备份/恢复 OpenAI/ChatGPT 凭据

## v0.2.2

- Node.js 24+、Rust stable 工具链、`toml` 1.x
- 依赖与 CI 工作流更新

## v0.2.1

- 真实编码 Agent CLI 集成测试（UAT）
- 四种路由策略 × 七个 Agent 端到端验证

## v0.2.0

- 持久化 `~/.cab/state.json`（Agent 与路由状态）
- 网关 Bearer 认证（`gateway_key`、`auth_enabled`）
- 新增 `cab-services` 应用层
- JSONL 请求日志与保留策略
- `POST /api/routing/explain` 与路由预览 UI
- Agent 与协议插件/适配器重构

## v0.1.x

- 首发：面向编码 Agent 的本地 LLM 网关
- 七个 Agent 集成
- 中英双语桌面 UI
- Windows、macOS、Linux 安装包

在 [GitHub Releases](https://github.com/xiongdi/cab/releases) 下载最新版本。
