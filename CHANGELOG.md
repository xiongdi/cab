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
