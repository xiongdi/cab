---
title: 快速开始
description: 五分钟完成安装、配置提供商并连接编码 Agent。
---

本指南覆盖核心工作流：安装 → 配置提供商 → 连接编码 Agent。

## 1. 启动 CAB

打开 CAB 桌面应用。本地网关自动在端口 **3125** 启动（可在设置中修改）。

默认端点：

```
http://127.0.0.1:3125/v1
```

## 2. 添加提供商

1. 在侧边栏打开 **提供商（Providers）**。
2. 等待 models.dev 目录同步（或点击刷新）。
3. 展开提供商行，添加至少一个 **API Key**。
4. 启用该提供商及需要参与路由的模型。

没有已启用的提供商和模型，路由无法转发请求。

## 3. 复制网关密钥

1. 打开 **设置（Settings）**。
2. 复制 **Gateway API Key**——自动/手动模式下的 Agent 用它作为 Bearer 令牌。
3. 默认开启认证（`auth_enabled: true`），每次网关请求需携带：

   ```
   Authorization: Bearer <gateway_key>
   ```

   通过 CAB 配置的 Agent 会自动注入该密钥。

## 4. 连接编码 Agent

1. 打开 **Agent** 页面。
2. 选择一个 Agent（如 Codex 或 Claude Code）。
3. 选择模式：
   - **自动**——CAB 改写 Agent 配置并绑定路由策略（推荐默认 `balanced`）。
   - **手动**——CAB 注册所有已启用模型，在 Agent 终端内自行选择。
   - **原生**——绕过 CAB（适合对比测试）。
4. 点击 **保存**。CAB 会备份并改写 Agent 配置文件。

详见 [Agent 模式](../../guides/agents/)。

## 5. 发送测试请求

照常运行 Agent CLI。CAB 拦截请求、排序模型并转发到最佳匹配。

在仪表盘 **日志** 页面确认路由到的提供商、模型、token 用量和延迟。

## 6. 调优路由（可选）

- 在 **Agent** 页面更换策略（auto / balanced / intelligent / price）。
- 在 **路由** 页面创建自定义规则与降级链。
- 使用 **路由 → 解释路由** 预览提示词如何被解析。

## 下一步

- [提供商与模型](../../guides/providers-and-models/)
- [路由策略](../../guides/routing/)
- [支持的 Agent](../../reference/supported-agents/)
