# Changelog

All notable changes to CAB are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

## [Unreleased]

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
