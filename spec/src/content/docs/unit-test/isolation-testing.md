---
title: 隔离测试
description: 验证 CAB 最小单元在隔离环境下运作
chapter: unit-test
order: 2
---

## 隔离原则

| 单元                          | 隔离方式                                        |
| ----------------------------- | ----------------------------------------------- |
| `rank_models`                 | 仅传入 `&[Model]` 与 `RequestProfile`，无 store |
| `effective_token_cost`        | 纯函数，models.dev 定价字段                     |
| `ordered_api_keys`            | 仅 `&[ApiKeyConfig]`                            |
| `subscription_quota`          | 固定 `chrono` 时间或解析 Header 字符串          |
| `protocol` 转换               | 内存 JSON `Value`，无 reqwest                   |
| `pick_endpoints_for_protocol` | 构造 `Provider` 结构体                          |

## 隔离测试用例

### UT-ISO-01：路由成本来自 models.dev 定价

构造同一 `Model`，修改 `input_cost` / `output_cost` / `cache_read`，断言 `effective_token_cost_for_model` 按 blended_input×10 + output 变化。

### UT-ISO-02：Key 顺序遵循配置顺序

构造 `api_keys` 数组，调整条目顺序或 `enabled` / `quota_reset_at`，断言 `ordered_api_keys` 按配置顺序返回可用 Key，跳过 rate-limited 项。

### UT-ISO-03：协议转换无网络

`protocol.rs` 测试用例输入完整 request body JSON，断言输出字段映射正确，不启动 Tokio runtime 网络。

### UT-ISO-04：URL 规范化

`catalog_provider_urls` 测试仅处理字符串，不访问 models.dev。

## 非隔离边界

以下逻辑归入 **集成测试**（需 `InMemoryStore` 或多模组）：

- `resolve_route` 全链路
- `sync_models_dev_catalog` HTTP 拉取
- `apply_agent_config` 文件系统写入

## 分模组用例表

### cab-core/routing.rs

| 用例                 | 期望                     |
| -------------------- | ------------------------ |
| effective_token_cost | blended_input×10+output  |
| 已知免费定价         | cost = 0，value = +∞     |
| Auto 高复杂度        | 低 capability 模型被过滤 |

### cab-core/subscription_quota.rs

| 用例                | 期望                       |
| ------------------- | -------------------------- |
| Retry-After 秒/日期 | 正确 Duration              |
| is_key_rate_limited | quota_reset_at 未来为 true |

### cab-gateway

| 模组        | 用例                      |
| ----------- | ------------------------- |
| router.rs   | pick_endpoints 协议优先   |
| protocol.rs | OpenAI↔Anthropic 字段映射 |

```bash
cargo test -p cab-core -p cab-gateway -p cab-api
```

## 通过标准

隔离用例无 `#[ignore]` 失败；无测试间共享可变全局状态。
