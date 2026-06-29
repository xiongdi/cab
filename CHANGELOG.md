# Changelog

All notable changes to CAB are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.5.1] - 2026-06-29

### Added

- **Auto-Update & Check-Update**. Added background checking and one-click update installation on the Svelte dashboard, pulling release assets directly from GitHub releases.
- **DeepSeek Prompt Cache Optimization & Realignment**. Added automatic extraction of dynamic parameters (`gitStatus` and `currentDate`) from the system prompt, appending them at the end of the messages history for OpenAI-compatible (DeepSeek) endpoints. This ensures the massive system prompt prefix is 100% static and hits the cache.
- **Protocol Priority Routing**. Native client protocol matching takes first priority during endpoint resolution to avoid translation when native endpoints are available.

## [0.5.0] - 2026-06-26

### Added

- **Prompt-cache hit optimization — session affinity** (`cache_affinity_enabled`, default on). Pins a conversation to the provider + model it first resolved to, so an upstream prefix cache keeps hitting across turns instead of cold-starting when the router would otherwise re-score to a different target. The pin is re-evaluated only when its target becomes unavailable. Toggle in **Settings**.
- **Cache-aware request shaping** (`cache_request_shaping_enabled`, default on). Rewrites the forwarded body for prefix-cache friendliness without changing request semantics: tool definitions are deterministically ordered (by name, then full schema) so client-side reordering no longer busts the cache, and Anthropic `cache_control` breakpoints are injected over the tools + system prefix when forwarding to an Anthropic endpoint and the client set none. Toggle in **Settings**.
- **Cache observability & costing.** Request logs now record `cache_read_tokens` / `cache_creation_tokens` (non-streaming and streaming), the Logs page shows a per-request cache-hit %, and `cost_usd` is computed from model pricing including cache read/creation rates (previously hardcoded to `0`).
- **Prompt-cache miss diagnostics.** Per-session prefix-shape tracking emits a gateway log explaining _why_ a cache likely went cold (system prompt vs. tool schemas changed) between turns.
- **Tool-schema weight diagnostics.** New `GET /api/diagnostics/tool-weights` endpoint and a Logs-page panel surface per-tool estimated token cost (heaviest first) so expensive tool schemas in the cacheable prefix are visible and prunable.

## [0.4.1] - 2026-06-25

### Security

- **Management API authentication bypass closed.** The `Origin`/`Referer` same-origin shortcut for `/api/*` is now additionally gated on the connection originating from loopback. A `host = "0.0.0.0"` / LAN deployment can no longer be reached unauthenticated via a forged `Referer`/`Origin` header — remote callers must present the `gateway_key`. The local browser dashboard on `127.0.0.1` is unaffected.
- **Provider credential fragments no longer leak.** Upstream provider error bodies (which occasionally echo partial API keys) are now scrubbed before being returned to gateway clients and before being persisted in request logs.
- **Endpoint URL validation.** Provider endpoint updates reject non-`http(s)` URLs (e.g. `file://`, `gopher://`), reducing the SSRF surface of the upstream forwarder. Self-hosted / LAN endpoints remain allowed.
- **CORS tightened.** The management API now reflects only trusted local dashboard origins (`localhost`/`127.0.0.1`/`[::1]`/`tauri`) instead of `*`.

## [0.4.0] - 2026-06-24

### Added

- **Reasonix agent integration** (`esengine/DeepSeek-Reasonix`): new CAB-managed coding agent configured at `~/.reasonix/config.toml` (secrets in `~/.reasonix/.env`), with native / auto / manual modes. Auto mode injects a `cab` OpenAI-compatible provider pointing at the gateway; manual mode exposes every enabled model.
- **Real brand icons for all coding agents** in the dashboard Agents page, served from `static/agent-icons/` (Claude Code, Codex, OpenCode, Hermes, Kilo Code, OpenClaw, Pi, Reasonix), replacing the generic placeholder line icons.

### Fixed

- **Newly-supported agents now appear after upgrade.** Agent state load merges persisted agents over the seeded defaults instead of overwriting them, so agents added in a new version (e.g. Reasonix) show up on existing installs, while user mode/model choices are preserved and removed agents are dropped.
- **Self-healing model catalog.** `load_catalog_models` now purges `catalog_models` rows that no longer match the `Model` schema (legacy orphans from older versions) and logs the concrete deserialization reason, instead of silently warning on every startup — eliminating the recurring `Skipping invalid model data` log spam and keeping the table from accumulating stale rows.
- **Request logs attribute Reasonix correctly.** Reasonix sends no identifying headers and no custom `User-Agent` on LLM requests (Go's default `Go-http-client/1.1` leaks through; its OpenAI providers can't carry custom headers — upstream esengine/DeepSeek-Reasonix#3824), so the gateway now maps a bare Go `User-Agent` to `reasonix` as a last-resort fallback instead of logging it as `unknown`.

### Changed

- App version in `src-tauri/tauri.conf.json` synced to the workspace version (was stale at `0.2.7`).

## [0.2.0] - 2026-06-10

### Added

- Persistent `~/.cab/state.json` for agents and routes (survives restart).
- Gateway and management API Bearer auth (`auth_enabled`, random `gateway_key` on first install).
- New `cab-services` application layer crate.
- JSONL request logs under `~/.cab/logs/` with retention policy.
- `POST /api/routing/explain` and Routes page routing preview.
- OpenAPI spec and frontend type generation scripts.

### Changed

- Architecture: Gateway/API → cab-services → cab-db/cab-core.
- Agent integrations and protocol handlers refactored to plugin/adapter pattern.

### Migration

- Upgrading from v0.1.x: existing `settings.json` is preserved; `auth_enabled` defaults to `true`.
- First start after upgrade writes initial `state.json` from current agent defaults.
- All API and Gateway clients must send `Authorization: Bearer {gateway_key}` (Agents in auto mode receive this automatically).

## [0.3.0] - 2026-06-24

### Added

- **SQLite storage backend**: all persistent data (settings, state, catalog, logs, usage records) consolidated into a single `~/.cab/cab.db` file. Removed `settings.json`, `state.json`, `catalog/*.json` files, and JSONL log files.
- **Database schema migration** (v1→v2): automatic one-time import of existing `catalog.json` cache on first startup.
- **Agentic routing strategy** (`agentic`): routes to models with the highest `agentic_index` score.
- **`GET /api/usage` endpoint** and **Usage page** in the dashboard for per-model and per-provider token usage analytics.
- **Health check module** (`cab-core::health`) for internal system health diagnostics.
- **API type generation** (`src/lib/api-types.ts`) for frontend TypeScript integration.
- **OpenAPI spec** expanded with usage, health, and catalog status endpoints.

### Changed

- **Unified routing strategy rankings**: all five strategies now use positive-semantic primary/secondary metric pairs with per-strategy comparator directions. No more encoding tricks (`-cost`, `-time`). Route explainer displays raw metrics with unit suffixes.
- **Speed strategy** now uses AA-style "Total Response Time for N Output Tokens" (`TTFT + 1000/tps`) as a single composite primary metric instead of separate speed→TTFT→cost tiebreaks.
- **Cheapest strategy** secondary key is now `overall_intelligence` (was `coding_index`).
- **Request-aware routing** now estimates output tokens from request body to compute a dynamic input:output ratio for value scoring.
- **Route candidate ranking** unified: subscription pool distinction removed; all candidates ranked by the same strategy comparator.
- Database file permissions restricted to `0600` (owner-only) and directory to `0700` to protect gateway_key and provider API keys.
- `.cab/` directory created with restricted permissions on first run.

### Fixed

- Removed unused `_path` parameter from `sync_models_dev_json` and cleaned up dead imports.
- Updated stale doc comments referencing `settings.json` to reflect SQLite storage.

### Migration

- **Breaking**: `~/.cab/settings.json`, `~/.cab/state.json`, `~/.cab/catalog/*.json`, and `~/.cab/logs/*.jsonl` are no longer used. On first startup after upgrade, catalog data is automatically imported from the old cache files (if present). Settings and state are re-initialized from defaults.
- Existing `gateway_key` is regenerated on first SQLite startup (update agent configs accordingly).
- All clients continue to use `Authorization: Bearer {gateway_key}` — no protocol changes.

## [Unreleased]

## [0.2.7] - 2026-06-15

### Added

- **IR-based gateway protocol engine**: Anthropic Messages, OpenAI Chat, and OpenAI Responses now convert through a shared intermediate representation with unified SSE streaming.
- **Cross-protocol fallback shims** and strategy-aware route-resolver fallbacks when the preferred endpoint protocol is unavailable.
- Routes page **value score** shows **∞** for models with a known **$0 endpoint price**; explain API adds `value_unbounded` for JSON consumers.
- Agent configs for **pi**, **opencode**, and **openclaw** now include the **speed** routing strategy.

### Changed

- **Balanced / Auto value score** uses **endpoint** pricing (what you pay through the service provider), not catalog list price, so subscription $0 rows rank correctly.
- Free models (`cost == 0`) get **+∞** value with tie-break on capability, then cost — no more `0.001` floor on value scores.

### Fixed

- **SSE stream ordering**: `finish_reason` is emitted before `[DONE]` (fixes pi agent `Stream ended without finish_reason`).
- **Codex 0.134+**: managed auth now writes a placeholder `id_token` when backing up/restoring `auth.json`.

## [0.2.6] - 2026-06-14

### Added

- **`GET /api/models/routable`**: lists enabled models with the **service provider** that would actually serve requests (native vendor or enabled reseller gateway such as OpenCode Go).
- **Routability layer** (`cab-db::routability`): resolves reseller endpoints, suffix-matches bare model slugs to canonical catalog IDs, and drives routing for models enabled only on gateway providers.
- Routes page **strategy metric columns**: composite price (Cheapest) and value score (Balanced); provider column shows the serving gateway, not the model vendor.
- **`data-revision`** store so Models/Providers toggles refresh Routes candidates without a full reload.

### Changed

- **Effective token cost** for Auto / Balanced / Cheapest: **10:1** input:output weighting with **90%** assumed prompt-cache hit rate when `cache_read` pricing exists (`blended_input×10 + output`).
- Routing resolver and OpenAI model list accept reseller-routable models when the native vendor is disabled.
- UI copy: unified **提供商** label; strategy descriptions updated for the new cost formula (EN / 简体中文).

### Fixed

- Reseller-only enabled models (e.g. DeepSeek V4 via OpenCode Go) now appear in routing candidates and resolve correctly at request time.

## [0.2.5] - 2026-06-11

### Added

- **Speed** routing strategy (`speed`): routes to the fastest AA median output speed among enabled models; ties break on lower TTFT, then cost; falls back to **Price** when no speed data is available.
- AA catalog sync now stores performance metrics (`median_output_tokens_per_second`, `median_time_to_first_token_seconds`) on models.
- Models page shows AA output speed and time-to-first-token when available.

### Changed

- Routing docs (EN / 简体中文) and Agents UI include the new Speed strategy.

## [0.2.4] - 2026-06-11

### Added

- Official documentation site at [xiongdi.github.io/cab](https://xiongdi.github.io/cab/) (Astro + Starlight, bilingual EN / 简体中文).
- GitHub Pages deployment workflow (`.github/workflows/docs.yml`).
- Product docs: quick start, routing, agents, providers, gateway auth, architecture, and API reference.

### Changed

- README and release notes now link to the official site instead of in-repo markdown guides.

## [0.2.3] - 2026-06-10

### Fixed

- **Codex**: dynamic authentication via `auth.json` (using ChatGPT OAuth `access_token` mechanism) when in CAB managed modes, eliminating the need to configure `OPENAI_API_KEY` system environment variables.
- **Codex**: automatic backup of existing OpenAI/ChatGPT login settings and credentials upon enabling managed mode, and seamless restoration when returning to native mode.

## [0.2.2] - 2026-06-10

### Changed

- Node.js requirement raised to **24+** (LTS); CI and `.nvmrc` updated.
- Rust toolchain pinned via `rust-toolchain.toml` (`stable`).
- `toml` crate upgraded from 0.9 to **1.x** (Codex config generation).
- GitHub Actions: `actions/checkout@v6`, `actions/upload-artifact@v7`.
- Rust and npm dependencies refreshed (`uuid`, `http-body-util`, `tempfile`, SvelteKit, `@tauri-apps/api`, etc.).

## [0.2.1] - 2026-06-10

### Added

- Local UAT now starts the **release** `cab-server` binary and invokes **real coding-agent CLIs** (UAT-10/11/12).
- UAT Markdown reports under `reports/uat/` with per-case pass/fail summary.
- `scripts/uat/` helpers: packaged server lifecycle, `run-real-ca.sh` per-agent probes.
- UAT-11 covers all four auto strategies (`auto`, `balanced`, `intelligent`, `price`) × seven agents.
- Expanded route-resolver tests for built-in `balanced` strategy (subscribed vs pay-as-you-go).

### Changed

- `./scripts/run-uat.sh` builds release, waits for catalog sync, tears down managed server on exit.
- UAT tests connect to `CAB_UAT_BASE_URL` instead of an in-process ephemeral server.
- Anthropic UAT (UAT-08) matches providers with enabled Anthropic endpoints, not only `protocol=anthropic` models.

### Fixed

- Real-CA verification when in-memory request logs hit the 500-entry ring buffer (CLI success no longer false-fails).

## [0.1.3] - 2026-06-09

### Added

- Gateway layer now recognizes and forwards requests for all seven supported coding agents (Claude Code, Codex, OpenCode, Hermes, Kilo Code, OpenClaw, Pi).
- `cab-core` benchmark catalog, subscription quota tracking, and expanded config surface.
- New `cab-api` modules for agents, benchmarks, models, providers, and settings endpoints.
- New `cab-db` modules for dashboard, endpoint, log, model, provider, route, and settings storage.
- New `cab-gateway` modules for agent identification, Anthropic protocol translation, OpenAI protocol translation, fallback routing, HTTP proxy, and protocol abstraction.
- Frontend coverage runner (`scripts/run-coverage.mjs`) wired through `package.json` `coverage:*` scripts.

### Changed

- Working tree synced to `main`: all workspace crates, frontend, docs, spec site, and CI workflows aligned to the latest schema.
- README and install docs refreshed; bilingual install guides now live at `docs/INSTALL.md` and `docs/INSTALL.zh-CN.md`.

### Fixed

- Resolved `clippy::bool_comparison` in `crates/cab-db/src/endpoint.rs` test (`== false` → negation).
- Resolved `clippy::bool_assert_comparison` in `crates/cab-db/src/model.rs` test (`assert_eq!(x, true)` → `assert!(x)`).

## [0.1.2] -2026-06-09

### Added

- Bilingual desktop UI (English /简体中文) across dashboard, routes, models, logs, and shared components.
- Windows WiX installers in both `en-US` and `zh-CN` (`*_x64_en-US.msi`, `*_x64_zh-CN.msi`, and ARM64 variants).
- NSIS installer language selector (English / Simplified Chinese).

### Changed

- Sidebar and layout now show the release version dynamically.

## [0.1.1] -2026-06-08

### Added

- Vite+ toolchain migration (`vite-plus`, unified `vp` scripts).
- Layered test gate in CI: UT → IT → ST via `scripts/run-tests.sh`; UAT isolated to `scripts/run-uat.sh`.
- `cab-server` library surface and expanded integration / system test coverage.

### Changed

- CI now enforces `rustfmt`, `clippy`, `vp check`, `svelte-check`, and `vp test` before release builds.
- README consolidated to English with a dedicated Chinese doc at `docs/README.zh-CN.md`.

## [0.1.0] -2026-06-01

### Added

- Initial release: local LLM gateway router for coding agents.
- OpenAI / Anthropic protocol gateway on `http://127.0.0.1:3125/v1`.
- Cost- and capability-aware routing with `models.dev` catalog sync.
- Tauri + Svelte desktop dashboard for providers, routes, agents, and logs.
- Agent config switcher for Claude Code, Codex, OpenCode, Hermes, Kilo Code, OpenClaw, and Pi.
- Desktop installers for Windows, macOS, and Linux.
