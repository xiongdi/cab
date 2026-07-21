---
title: Gateway & Auth
description: CAB gateway endpoints, authentication, and local configuration.
---

CAB exposes a local HTTP gateway compatible with OpenAI and Anthropic client SDKs, plus a management API for the dashboard.

## Gateway endpoints

Default base URL:

```
http://127.0.0.1:3125/v1
```

| Endpoint                    | Protocol  | Purpose                            |
| --------------------------- | --------- | ---------------------------------- |
| `POST /v1/chat/completions` | OpenAI    | Chat completions (most agents)     |
| `POST /v1/messages`         | Anthropic | Anthropic Messages API             |
| `POST /v1/responses`        | OpenAI    | Responses API                      |
| `GET /v1/responses`         | OpenAI    | Responses over WebSocket           |
| `GET /v1/models`            | OpenAI    | List routable models (Manual mode) |

CAB identifies the calling agent from the User-Agent header and applies the matching route or strategy.

## Authentication

Gateway auth is **enabled by default**:

```
Authorization: Bearer <gateway_key>
```

The gateway also accepts `x-api-key: <gateway_key>` (Bearer wins if both are present).

- `gateway_key` is generated on first install and stored in SQLite `~/.cab/cab.db` (`settings` row, `id = 1`).
- View or regenerate it in **Settings → Gateway API Key**.
- Agents configured through CAB in Auto/Manual mode receive the key automatically.
- External clients must send the header manually.

`auth_enabled` can be toggled in settings, but keeping it on is recommended for local security.

## Configuration storage

| Location          | Contents                                                            |
| ----------------- | ------------------------------------------------------------------- |
| `~/.cab/cab.db`   | Settings (port, gateway key, auth), agents, routes, request logs, … |
| `cab.toml`        | Bootstrap host + first-install port seed (not API-editable)         |
| `~/.cab/catalog/` | models.dev / related download cache                                 |

Deprecated (not runtime config): `~/.cab/settings.json`, `~/.cab/state.json`, `~/.cab/logs/*.jsonl`.

## Port changes

The default port is **3125**. Changing it in Settings requires a CAB restart. Update agent configs if you use a custom port.

## Protocol conversion

When a model's native protocol differs from what the agent sends (e.g. Anthropic-only model called via OpenAI protocol), CAB converts at the gateway layer and forwards to the best matching endpoint.

## Headless server

Run without the desktop UI (for release testing or production):

```bash
cargo run -p cab-srv
```

The headless daemon serves the same gateway and management API. The built UI is also available as static files from the server.

> For daily development, use `npm run dev:server` (cargo watch with hot reload) instead — see [AGENTS.md](https://github.com/xiongdi/cab/blob/main/AGENTS.md).

## Related

- [API reference](../../reference/api/) — management API endpoints
- [Architecture](../../reference/architecture/) — gateway crate overview
