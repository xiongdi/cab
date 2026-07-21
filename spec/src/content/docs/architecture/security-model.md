---
title: 安全模型
description: CAB localhost 信任模型与 Gateway 鉴权
chapter: architecture
order: 4
---

## 威胁模型

CAB 面向**单用户本地部署**。默认绑定 `127.0.0.1`，假定同一主机上的进程可信。主要风险：

- 同机其他进程未授权调用 Gateway（消耗 API 额度）
- 管理 API 被本地恶意页面跨域调用（CORS 已放开 Origin）

## 鉴权策略

| 设置项         | 默认值    | 说明                                 |
| -------------- | --------- | ------------------------------------ |
| `auth_enabled` | `true`    | 关闭后跳过 Bearer 校验（仅建议调试） |
| `gateway_key`  | 随机 UUID | 首次安装写入 SQLite `settings` 时生成 |

## 请求契约

所有 `/v1/*` 与 `/api/*` 请求（`auth_enabled == true` 时）：

```http
Authorization: Bearer {gateway_key}
```

- 缺失或错误 → `401 Unauthorized`
- 静态前端资源（`/_app/*`、`index.html`）不鉴权

## Agent 配置同步

`apply_agent_config` 在 Agent 设为 `auto`/`manual` 时，将 CAB 端点与 `gateway_key` 写入各 Agent 配置文件，确保 CLI 请求自动携带正确 Bearer。

## 密钥存储

- `gateway_key` 与上游 API Key 均明文存于 `~/.cab/cab.db`（本地信任模型）
- 不在日志或 OpenAPI 示例中输出真实密钥

## 网络绑定

- `cab.toml` 默认 `gateway.host = "127.0.0.1"`
- 若用户改为 `0.0.0.0`，必须在文档与 UI 中警告：须保持 `auth_enabled = true`
