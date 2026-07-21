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

| Location                                     | Contents                                                            |
| -------------------------------------------- | ------------------------------------------------------------------- |
| `$CAB_HOME/cab.db` (default `~/.cab/cab.db`) | Settings (port, gateway key, auth), agents, routes, request logs, … |
| `$CAB_HOME/service.json`                     | Installed service scope (`user` / `system`)                         |
| `cab.toml`                                   | Bootstrap host + first-install port seed (not API-editable)         |
| `$CAB_HOME/catalog/`                         | models.dev / related download cache                                 |

Deprecated (not runtime config): `~/.cab/settings.json`, `~/.cab/state.json`, `~/.cab/logs/*.jsonl`.

## Port changes

The default port is **3125**. Changing it in Settings requires a CAB restart. Update agent configs if you use a custom port.

## Protocol conversion

When a model's native protocol differs from what the agent sends (e.g. Anthropic-only model called via OpenAI protocol), CAB converts at the gateway layer and forwards to the best matching endpoint.

## Headless server / daemon

`cab-srv` is the **sole** HTTP server (gateway + API + static UI). Install it as a background service with a **user** or **system** scope:

```bash
cab-cli service install --scope user    # default: login session, data in ~/.cab
sudo cab-cli service install --scope system  # boot-time; needs admin/root
cab-cli start
```

| Scope    | When it runs                           | Data directory                                                                              | Privilege / account               |
| -------- | -------------------------------------- | ------------------------------------------------------------------------------------------- | --------------------------------- |
| `user`   | After user login (default)             | `~/.cab`                                                                                    | Normal user                       |
| `system` | At boot (before login where supported) | Linux `/var/lib/cab`; macOS `/Library/Application Support/cab`; Windows `%ProgramData%\cab` | Dedicated least-privilege account |

Platform mechanisms (hardened):

| Platform | user                                        | system                                                                                    |
| -------- | ------------------------------------------- | ----------------------------------------------------------------------------------------- |
| Linux    | `systemd --user` + linger                   | system unit as user `cab`, `ProtectSystem=strict`, …                                      |
| macOS    | LaunchAgent                                 | LaunchDaemon as `_cab` when creatable                                                     |
| Windows  | Task Scheduler ONLOGON + restart-on-failure | SCM service as `NT AUTHORITY\LocalService`, env via service registry (not machine `setx`) |

Scope is recorded in `service.json` under the data dir (and a user-home pointer for discovery). Override the data root with `CAB_HOME`. System installs set `CAB_HOME` in the unit/plist/service environment.

`cab-gui` is a thin client: on first run (if no service is installed) it prompts for scope, then starts `cab-srv` and opens `http://127.0.0.1:{port}/`. Closing the GUI leaves the daemon running.

Do **not** bind the gateway to `0.0.0.0` for public exposure — keep host from `cab.toml` (default `127.0.0.1`).

> For daily development, use `npm run dev:server` (cargo watch with hot reload) instead — see [AGENTS.md](https://github.com/xiongdi/cab/blob/main/AGENTS.md).

## Related

- [API reference](../../reference/api/) — management API endpoints
- [Architecture](../../reference/architecture/) — gateway crate overview
