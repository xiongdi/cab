---
title: 用户环境验证
description: 在用户实际环境中验证 CAB
chapter: acceptance
order: 2
---

## 目标环境

| 项   | 典型配置                                  |
| ---- | ----------------------------------------- |
| OS   | Linux / macOS / Windows（Tauri 支持平台） |
| 网络 | 可访问上游 LLM API 与 models.dev          |
| 磁盘 | 可写 `~/.cab`、Agent 配置目录             |
| 权限 | 普通用户权限可运行本地网关                |

## 环境检查清单

- [ ] `curl http://127.0.0.1:3125/api/settings` 返回 JSON
- [ ] 浏览器打开管理 UI（build 或 dev）
- [ ] `~/.cab/settings.json` 随 UI 操作更新
- [ ] 目标 Agent 已安装且可配置 base URL

## 与实验室差异

| 差异点   | 用户环境注意                            |
| -------- | --------------------------------------- |
| 防火墙   | 仅本机端口，一般无影响                  |
| 企业代理 | 上游 HTTPS 可能需系统代理（CAB 未内置） |
| 多用户   | settings 按 OS 用户隔离                 |
| Key 类型 | 用户实际订阅/按量混合                   |

## 桌面 vs CLI

| 模式    | 入口     | 适用         |
| ------- | -------- | ------------ |
| cab-srv | 终端启动 | 服务器式使用 |
| Tauri   | 桌面图标 | 日常开发者   |

两者共享同一套 `InMemoryStore` 与路由逻辑。

## 验收记录

记录 OS 版本、CAB 版本（git tag）、gateway_port、使用的 Agent 与提供商。
