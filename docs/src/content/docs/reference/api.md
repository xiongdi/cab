---
title: API
description: CAB gateway and management API overview.
---

CAB exposes two API surfaces: the **gateway** (OpenAI/Anthropic-compatible, for agents) and the **management API** (REST, for the dashboard).

## Gateway API

Base: `http://127.0.0.1:3125/v1`

Authenticated with `Authorization: Bearer <gateway_key>` (also accepts `x-api-key`).

| Method | Path                   | Description             |
| ------ | ---------------------- | ----------------------- |
| `POST` | `/v1/chat/completions` | OpenAI chat completions |
| `POST` | `/v1/messages`         | Anthropic messages      |
| `POST` | `/v1/responses`        | OpenAI responses        |
| `GET`  | `/v1/responses`        | Responses over WebSocket |
| `GET`  | `/v1/models`           | List routable models    |

Agents identify themselves via User-Agent; CAB uses this for route matching.

## Management API

Base: `http://127.0.0.1:3125/api`

Also requires Bearer auth when `auth_enabled` is true (loopback dashboard Origin/Referer may bypass).

| Area           | Endpoints                                      | Purpose                                                    |
| -------------- | ---------------------------------------------- | ---------------------------------------------------------- |
| **Settings**   | `GET/PUT /api/settings`                        | Port, gateway key, auth, retention                         |
| **Settings**   | `GET /api/settings/catalog-status`             | Catalog sync status                                        |
| **Settings**   | `POST /api/settings/sync-catalog`              | Trigger catalog sync                                       |
| **Providers**  | `/api/providers/*`                             | Provider catalog and key management                        |
| **Models**     | `/api/models/*`, `PUT /api/model-endpoints`    | Model catalog, routable/catalog lists, endpoints           |
| **Routes**     | `/api/routes/*`                                | Custom routing rules                                       |
| **Agents**     | `/api/agents/*`                                | Agent mode and strategy config                             |
| **Logs**       | `GET/DELETE /api/logs`                         | Request log query / clear                                  |
| **Usage**      | `GET /api/usage/summary`, `/api/usage/records` | Usage aggregates and records                               |
| **Routing**    | `POST /api/routing/explain`                    | Preview routing decision for a prompt                      |
| **Routing**    | `POST /api/routing/strategy-board`             | Full ranked candidates per built-in strategy               |
| **Diagnostics**| `GET /api/diagnostics/tool-weights`            | Tool-weight diagnostics                                    |
| **Dashboard**  | `GET /api/dashboard/stats`                     | Stats and health                                           |
| **Update**     | `GET /api/update/check`, `POST /api/update/install` | App update check / install                            |
| **Logos**      | `GET /api/logos/{*path}`                       | Provider logo assets                                       |

An OpenAPI spec is maintained in the repository (`spec/`). Generate frontend types with the project scripts.

## Routing explain

`POST /api/routing/explain` accepts an agent ID, optional model/strategy, and a sample message. Returns:

- Resolved target (provider + model)
- Decision steps
- Ranked candidate list

This powers the **Routes → Explain routing** preview in the dashboard.

## Strategy board

`POST /api/routing/strategy-board` accepts an agent ID and sample message body. Returns fully ranked candidate lists for all six built-in strategies (`auto`, `balanced`, `cheapest`, `intelligent`, `speed`, `agentic`). Ranking matches gateway `cab-core::routing`; the Routes page strategy tables consume this API only (no duplicate client-side sort).

## Related

- [Gateway & Auth](../../guides/gateway-auth/) — authentication details
- [GitHub repository](https://github.com/xiongdi/cab) — full OpenAPI spec and source
