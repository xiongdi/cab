# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this project is

**CAB (Coding Agents Bridge)** — a local, cost-aware LLM gateway router for coding agent CLIs. Agents point at `http://localhost:3125/v1`; CAB ranks providers/models (Intelligence / Coding / Agentic indices + token pricing + context window) and forwards requests upstream to OpenAI / Anthropic. Syncs model/pricing/benchmark data from `models.dev` in real time. Ships a Tauri + Svelte desktop dashboard for managing providers, keys, routing strategies, and per-agent config (Claude Code, Codex, OpenCode, Hermes, Kilo Code, OpenClaw, Pi, Reasonix).

## Workspace layout

Cargo workspace (`crates/*`, `src-tauri`) + Svelte/Tauri frontend (`src/`, `src-tauri/`).

| Crate          | Role                                                                                                           |
| -------------- | -------------------------------------------------------------------------------------------------------------- |
| `cab-core`     | Types, request profiling, routing/scoring algorithm                                                            |
| `cab-db`       | `~/.cab/cab.db` SQLite store (settings, agents, routes, request logs, catalog, usage)                          |
| `cab-services` | Catalog sync, route resolution, agent config switcher                                                          |
| `cab-gateway`  | Gateway auth, protocol adapters (`/v1/chat/completions`, `/v1/messages`, `/v1/responses`), upstream forwarding |
| `cab-api`      | Management REST API (`/api/*`)                                                                                 |
| `cab-srv`      | Headless daemon combining gateway + API + static UI (`crates/cab-server`)                                      |
| `cab`          | CLI binary `cab-cli`                                                                                           |
| `src/`         | Svelte dashboard (consumed by both Tauri and `cab-srv`)                                                        |
| `src-tauri/`   | Tauri shell                                                                                                    |

Runtime state lives at `~/.cab/cab.db` (SQLite). Agent configs (e.g. `~/.claude/settings.json`) are rewritten when switching modes (Native / Auto / Manual).

## Dev environment — strict rules

`AGENTS.md` is authoritative. **Read it before starting any dev work.** Key invariants:

- **Ports are globally unique on the host** — backend **3125**, frontend **5173**, both `strictPort`. Never change them; never run a second instance.
- **Only allowed dev commands:** `npm run dev` (frontend, terminal B) + `npm run dev:server` (backend watch, terminal A). Two processes total.
- **Forbidden:** `cargo run -p cab-srv`, `npm run dev:server:once`, `npm run tauri:dev`, manually running `target/**/cab-srv.exe`, stacking a second Vite/cargo-watch, or changing ports to dodge a conflict.
- **Port occupied → kill first.** PowerShell: `scripts/kill-dev-ports.ps1` (both) or `-Backend` (3125 only). Then verify with `netstat -ano | findstr "5173 3125"`.
- `gateway_port` stays 3125. Agent CLIs must set `ANTHROPIC_BASE_URL=http://localhost:3125`.

## Commands

### Build / run / check

```bash
# First time
npm install
cargo install cargo-watch          # for dev:server

# Daily dev (two terminals)
npm run dev                        # Svelte/Vite on :5173
npm run dev:server                 # cargo watch → cab-srv on :3125

# Tauri desktop (separate from the dev flow above — only when explicitly needed)
npm run tauri:dev

# Headless server only (NOT for daily dev — conflicts with dev:server)
cargo run -p cab-srv
```

### Code quality

```bash
cargo fmt --all -- --check         # Rust format check
cargo fmt --all                    # Rust format fix
cargo clippy --workspace --all-targets -- -D warnings
npm run check                      # Svelte + TypeScript
```

### Tests

```bash
cargo test --workspace             # all Rust tests
cargo test -p <crate>              # single crate, e.g. cargo test -p cab-core
cargo test -p <crate> <test_name>  # single test
npm run test:unit                  # frontend unit tests (vitest)
npm run coverage                   # frontend + backend coverage
```

### Real-world integration test (required before reporting "done")

`scripts/test-cc-headless.ps1` — drives the real Claude Code CLI in headless mode through the gateway. Per `AGENTS.md`, **integration validation must use the real agent CLI**, not curl/mock. Before running: ensure only one process owns 3125 (`kill-dev-ports.ps1 -Backend` if needed, then `npm run dev:server`). Hard timeout (default 120s) must kill `claude.exe` on expiry.

## Change → verify → report workflow

After **any** code or config change, before reporting back:

1. Clean: `scripts/kill-dev-ports.ps1` + stop stray `claude,cab-srv,cargo-watch`.
2. Start the unique dev pair: `npm run dev:server` (wait for catalog sync + 3125 LISTENING), then `npm run dev` (5173 LISTENING).
3. Sync `gateway_key` from SQLite `settings` table into `~/.claude/settings.json` `ANTHROPIC_AUTH_TOKEN` if keys changed.
4. Run the full verification checklist in `AGENTS.md` §"最小验证清单" (providers, `/api/routing/explain`, `/v1/messages`, frontend 200, headless CC, settings intact).
5. Report pass/fail with concrete evidence (port status, routing result, CC output, gateway logs from `GET /api/logs?per_page=3` on failure). Never report "should be fine" without verification.

## Configuration / data locations

- **`cab.toml`** (system bootstrap, NOT API-editable): `gateway.host` (default `127.0.0.1`), `gateway.port` (default `3125`, seeds database on first install)
- **`~/.cab/cab.db`** (SQLite database, user runtime, API-editable): settings (`gateway_port`, `gateway_key`, `auth_enabled`, `log_retention_days`, providers, models, …), plus tables for agents, routes, `request_logs`, usage, catalog, and AA benchmarks
- **`~/.cab/catalog/`**: models.dev / related cache files (not the primary config store)
- Agent configs rewritten by CAB: `~/.claude/settings.json`, `~/.codex/config.toml`, `~/.config/opencode/opencode.json`, `~/.hermes/config.yaml`, `~/.config/kilo/opencode.json`, `~/.pi/agent/models.json`, `~/.reasonix/config.toml` (+ `~/.reasonix/.env`); OpenClaw via `openclaw config`
- Docs source: `docs/` (published to GitHub Pages)
- Specs: `spec/` (API/protocol design notes)

Deprecated (do not use as runtime config): `~/.cab/settings.json`, `~/.cab/state.json`, `~/.cab/logs/*.jsonl`.

Port priority chain: SQLite `settings` `gateway_port` (runtime) → `cab.toml [gateway] port` (bootstrap fallback) → hardcoded `3125`. Host is always from `cab.toml`.

## Release / version

`Cargo.toml` `[workspace.package]` `version` and `package.json` `version` move together (currently **0.7.1**). Releases via GitHub Releases; `CHANGELOG.md` is maintained per release. PRs target `main`. See `CONTRIBUTING.md` and `.github/PULL_REQUEST_TEMPLATE.md`.
