---
title: 业务场景与样例
description: CAB 典型使用场景
chapter: system-design
order: 3
---

## 场景 SC-01：编程智能体自动路由

**触发**：Codex（或其他已支持 Agent）发送 `POST /v1/chat/completions`，`model` 为 `auto` 或 Agent 配置为 auto 模式。

**主路径**：

1. `extract_agent` 从 Header 识别 agent（如 `codex`）
2. `build_request_profile` 分析 messages 文本 → `TaskKind::Coding`
3. `rank_models` 在已启用模型中按能力/成本排序
4. 转发至最高分模型提供商

**源码**：`openai.rs` → `router.rs::resolve_by_strategy`

## 场景 SC-02：订阅额度用尽 fallback

**触发**：订阅 Key 收到 HTTP 429。

**路径**：

1. `proxy.rs` 解析 `Retry-After` → `CabError::ProviderError { retry_after }`
2. `mark_api_key_quota_reset` 写入 `quota_reset_at` 到 settings
3. `ordered_api_keys` 跳过限额内 Key，尝试按量 Key
4. 仍失败则 `fallback_models` 下一候选

**源码**：`fallback.rs`

## 场景 SC-03：Claude Code 无感接入

**触发**：用户在 Agents 页将 `claude-code` 设为 `auto`。

**路径**：

1. `PUT /api/agents/claude-code` → `apply_agent_config`
2. 写入 `~/.claude/settings.json` 指向 `http://127.0.0.1:{gateway_port}`
3. Claude Code 请求经 Gateway 路由

## 场景 SC-04：目录同步

**触发**：Settings 页点击同步，或启动时 `sync_models_dev_catalog`。

**路径**：拉取 models.dev → 更新 providers/models/endpoints → 应用用户 settings 覆盖。

## 场景 SC-05：重启恢复配置

**触发**：用户修改 Agent 为 `auto` 并创建 Route 后重启 cab-srv。

**路径**：

1. `init_store` 从 SQLite `~/.cab/cab.db` 水合 settings / agents / routes / catalog
2. agents/routes 合并到 `StoreData`
3. Dashboard 与 Agents 页显示重启前配置

**源码**：`cab-db/state.rs`、`init_store`

## 场景 SC-06：路由解释调试

**触发**：用户在 Routes 页填写 agent、model、messages 点击「模拟路由」。

**路径**：

1. `POST /api/routing/explain`
2. `route_explainer` 逐步记录 decision_steps
3. UI 展示命中策略与 ranked_candidates

**源码**：`cab-services/route_explainer.rs`、`routes/+page.svelte`
