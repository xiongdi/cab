---
title: API
description: CAB gateway and management API overview.
---

CAB exposes two API surfaces: the **gateway** (OpenAI/Anthropic-compatible, for agents) and the **management API** (REST, for the dashboard).

## Gateway API

Base: `http://127.0.0.1:3125/v1`

Authenticated with `Authorization: Bearer <gateway_key>`.

| Method | Path | Description |
| ------ | ---- | ----------- |
| `POST` | `/v1/chat/completions` | OpenAI chat completions |
| `POST` | `/v1/messages` | Anthropic messages |
| `POST` | `/v1/responses` | OpenAI responses |
| `GET` | `/v1/models` | List routable models |

Agents identify themselves via User-Agent; CAB uses this for route matching.

## Management API

Base: `http://127.0.0.1:3125/api`

Also requires Bearer auth when `auth_enabled` is true.

| Area | Endpoints | Purpose |
| ---- | --------- | ------- |
| **Settings** | `GET/PUT /api/settings` | Port, gateway key, auth, retention |
| **Providers** | `/api/providers/*` | Provider catalog and key management |
| **Models** | `/api/models/*` | Model catalog, enable/disable |
| **Routes** | `/api/routes/*` | Custom routing rules |
| **Agents** | `/api/agents/*` | Agent mode and strategy config |
| **Logs** | `/api/logs/*` | Request log query |
| **Routing** | `POST /api/routing/explain` | Preview routing decision for a prompt |
| **Routing** | `POST /api/routing/strategy-board` | Full ranked candidates per built-in strategy (Routes page) |
| **Dashboard** | `/api/dashboard/*` | Stats and health |

An OpenAPI spec is maintained in the repository (`spec/`). Generate frontend types with the project scripts.

## Routing explain

`POST /api/routing/explain` accepts an agent ID, optional model/strategy, and a sample message. Returns:

- Resolved target (provider + model)
- Decision steps
- Ranked candidate list

This powers the **Routes → Explain routing** preview in the dashboard.

## Strategy board

`POST /api/routing/strategy-board` accepts an agent ID and sample message body. Returns fully ranked candidate lists for all five built-in strategies (`auto`, `balanced`, `cheapest`, `intelligent`, `speed`). Ranking matches gateway `cab-core::routing`; the Routes page strategy tables consume this API only (no duplicate client-side sort).

## Related

- [Gateway & Auth](../../guides/gateway-auth/) — authentication details
- [GitHub repository](https://github.com/xiongdi/cab) — full OpenAPI spec and source
