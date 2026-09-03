# Changelog

All notable changes to CAB are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.11.9] - 2026-09-03

### Fixed

- Migrate legacy protocol names in existing SQLite state and cap upstream request latency at 60 seconds.

## [0.11.7] - 2026-09-03

### Changed

- Catalog synchronization now treats provider model bindings as authoritative, clears obsolete global snapshots, and preserves each OpenCode Go model's wire protocol.
- Added current OpenCode Go model protocol mappings and made catalog refresh fail atomically when the display cache cannot be refreshed.
- Release automation now selects the next available patch version automatically when a tag already exists.

## [0.11.5] - 2026-08-25

### Fixed

- **Atomic provider bulk model enable/disable**: `PUT /api/providers/{id}/models/enabled` updates bindings and `settings.models` in one pass. The provider page no longer fires concurrent per-model `PUT /api/models/{id}` calls that raced and left settings out of sync with catalog bindings (wrong candidate counts after “disable all / enable a few”).

### Changed

- **Models page is display-only from `models.dev/models.json`**: `/api/models/catalog` lists the canonical encyclopedia (~355 entries) instead of every provider binding. Reads local cache first; catalog sync also refreshes `~/.cab/catalog/models.dev/models.json`. Routing and provider bindings are unchanged.

## [0.11.4] - 2026-08-21

### Fixed

- **Anthropic→OpenAI Chat streaming usage/cache logging**: inject `stream_options.include_usage`, defer SSE finalize until the usage chunk arrives, and map `prompt_tokens` / cache legs into Anthropic `message_delta.usage`. Fixes Claude Code → hy3 (and similar Chat-native Go models) showing `input_tokens=0` and no cache hits after protocol conversion.

## [0.11.3] - 2026-08-20

### Changed

- **AA model map merges bundled baseline with user overlay**: the embedded map is always loaded first as the authoritative baseline, then the local `~/.cab/catalog/aa-model-map.json` is layered on top. New mappings shipped in releases now take effect for existing installs without losing user customizations.
- **Provider page model management overhaul**: models display in a detail grid with per-model enable/disable toggle switches, a "Disable all models" button, loaded-model count, and a hint for models not yet registered in the database. Endpoint and key delete buttons gain visible text labels and aria-labels for accessibility.

### Fixed

- **Gateway WS SSE parser**: satisfy clippy `collapsible_if` lint.

## [0.11.2] - 2026-08-20

### Fixed

- **Codex Responses WebSocket no longer falls back to HTTPS**: accept flat `response.create`, keep the socket across warmup/`generate:false` and multi-turn, emit full `response.*` event frames (via `responses_to_sse_stream`), cache `previous_response_id`, and Close cleanly.
- **Codex auto routing on WS**: write `http_headers.x-cab-agent = "codex"` and fall back when the upgrade omits a recognizable User-Agent for placeholder model `gpt-5.5`.

## [0.11.1] - 2026-08-20

### Changed

- **Auto task classification uses AA capability classes**: request profiling maps to `intelligence` / `coding` / `math` / `agentic` from structure (coding agent, code fences, tools / multi-turn, math notation) instead of keyword lists. `TaskKind::General` is renamed to `Intelligence`.
- **Catalog sync is provider-authoritative**: enabled provider model lists drive bindings; global models.dev rows stay reference metadata. Fixes OpenCode Go candidate counts that previously inflated via `merge_enabled_provider_models`.
- **Routing explain / strategy board show the full ranked pool**: display ranking skips the auto shortlist filter and no longer truncates to 10 — actual auto routing still uses the shortlist.
- **Artificial Analysis sync retains full model payloads**: `release_date`, `pricing`, evaluation extras (e.g. `terminalbench_v2_1`, `tau_banking`, `lcr`), and unknown top-level fields are preserved in the benchmark catalog.

### Fixed

- **Speed / cheapest strategies no longer rank models missing metrics first**: unroutable ASC scores use `+∞` so incomplete models sink to the bottom.
- **Grok Build manual mode sends full catalog ids** (e.g. `tencent/hy3`) instead of stripping the provider prefix to a bare suffix that the gateway cannot match.
- **Dashboard model get/update encodes ids with `/`** so enabling `meta/muse-spark-1.2-contributor` and similar names works from the Vite UI.
- **Vite dev proxies `/api` → `3125`** to avoid CORS when the Svelte UI on `5173` calls the management API.
- **AA map**: `muse-spark-1.2-contributor` / `meta/muse-spark-1.2-contributor` → `muse-spark-1-2`.
- **Routability pricing** can read provider `pricing` JSON when scalar `input_cost` / `output_cost` are absent.

## [0.11.0] - 2026-08-19

### Changed

- **Protocol handshake uses the model's native wire protocol, not endpoint priority**: OpenCode Go's Responses / Chat / Anthropic URLs no longer compete by numeric priority. Incoming requests sniff the client protocol and convert only when it differs from the model's native protocol.
- **OpenCode Go protocol lookup table**: bundled from the official [Go endpoints](https://opencode.ai/docs/go/) (`config/opencode-go-protocols.json`). Claude Code talking to `mimo-v2.5` hits `/v1/chat/completions` on the first hop; MiniMax/Qwen hit `/v1/messages`; Grok 4.5 and GPT 5.6 Luna hit `/v1/responses`. Unknown Go ids are family-sniffed (`kimi-*`, `qwen*`, …).

### Fixed

- **Claude Code no longer waits out a non-streaming Responses shim**: Messages→Responses (and other cross-protocol) conversions keep `stream=true` and translate official SSE. When upstream still returns a JSON body, synthesized Anthropic SSE now emits `thinking` / `tool_use` / usage instead of flattening to one text delta.
- **Anthropic `thinking.type: adaptive`** (Claude Code default) maps to Responses `reasoning.effort: medium` instead of being dropped.
- **Provider 401/403/404 on the wrong protocol** tries the next matching endpoint instead of failing the whole request (OpenCode Go `ModelError`).

## [0.10.11] - 2026-08-17

### Fixed

- **Agents page manual-mode hint showed 0 routable models**: the count only accepted the legacy `api_key` field and native-provider reachability, so providers whose keys live in the `api_keys` array (the panel's default save path) were treated as inactive and reseller-served models were invisible — the same mismatch already fixed for the dashboard. The hint now mirrors the backend rule: an enabled provider counts with a usable key in `api_keys` **or** `api_key` (ollama exempt), and models reachable via an enabled reseller endpoint of an active provider are counted too.

## [0.10.10] - 2026-08-17

### Fixed

- **Codex no longer hangs on Chat Completions models (e.g. mimo-v2.5)**: Codex streams the Responses API, but models like OpenCode Go `mimo-v2.5` only speak Chat Completions. CAB forced a non-streaming Chat fallback and synthesized SSE that emitted only a `message` item, so tool calls appeared only inside `response.completed`. Codex never executed tools and looked hung. The gateway now streams Chat SSE into the official Responses function-call lifecycle (`output_item.added` → `function_call_arguments.delta` → `function_call_arguments.done` with `name` → `output_item.done`), mapping Chat `tool_calls[].id` to `call_id` and generating a distinct `fc_*` item id. A Responses client also prefers the catalog protocol so Chat-only models are not POSTed to `/v1/responses` first. Claude Code still uses a native Anthropic endpoint when the catalog's `openai-chat` default would 401 (kimi-k3).

## [0.10.9] - 2026-08-16

### Fixed

- **Codex `view_image` fully supported (no more "Unsupported Image")**: two gaps blocked image-using Codex requests. (1) Vision detection only matched `type: image`/`image_url` and missed the `type: input_image` shape Codex actually emits inside `function_call_output.output`, so the request was routed to a text-only model that rejected the base64 payload. `contains_image` now recognizes `input_image`/`output_image` and `image_url` payloads. (2) Even after detection, the image was flattened to a JSON string instead of a real image block, so upstream models could not see it. `IrBlock::ToolResult` now carries mixed text+image content, and the gateway forwards `view_image` results as proper `image_url` parts to OpenAI-compatible and Anthropic upstreams (and round-trips the Responses `input_image` shape for Codex). Codex `view_image` requests now route to a vision-capable model and the image is delivered as a real image.

- **Frontend version label**: the dashboard sidebar showed `v0.10.7` because `package.json` was not bumped alongside the Rust version; it now matches the release.

## [0.10.8] - 2026-08-16

### Fixed

- **Codex `view_image` no longer routes to a text-only model**: Codex returns the image a `view_image` tool call reads as a base64 payload in the Responses protocol's `function_call_output.output` field. Vision detection only inspected `content`/`parts`/`text`/`type`, so the request was classified as text-only and routed to a non-vision model, which then rejected the image data (e.g. "不支持"). `function_call_output.output` is now recognized as image content, so image-bearing Codex requests route to a vision-capable model.

## [0.10.7] - 2026-08-16

### Fixed

- **Parallel tool calls through the OpenAI Chat fallback**: when a Responses request carried parallel `function_call`s (multiple consecutive calls before their outputs), the Responses→Chat conversion emitted consecutive `assistant` messages each with its own `tool_calls`, which upstream rejects with `An assistant message with 'tool_calls' must be followed by tool messages` (400). Adjacent assistant-with-tool_calls turns are now merged into one, so the sequence is `assistant(tool_calls:[A,B]) tool(A) tool(B)`.

## [0.10.6] - 2026-08-16

### Fixed

- **Gateway 413 on large request bodies**: the server defaulted to axum's 2 MB request-body limit, so oversized gateway requests (e.g. large tool-call payloads) could be rejected with `413 Payload Too Large`. The limit is raised to 100 MB.

## [0.10.5] - 2026-08-13

### Added

- **Provider “Enable all models”**: the Providers page detail panel now has a one-click button under the model list to enable every model associated with that provider.
- **`agentic` in agent auto-mode surfaces**: Agents strategy picker, `/v1/models` discovery, and OpenCode / Grok Build auto-mode config aliases now include `agentic` alongside `auto` / `balanced` / `intelligent` / `price` / `speed` (six built-in strategies).

### Changed

- **Agent-aware `/v1/models`**: in auto mode the gateway returns routing strategies instead of concrete models — Claude Code as `claude/cab/{strategy}`, OpenCode/Codex as `cab/{strategy}`, Grok Build as `cab-{strategy}`. Manual mode still lists enabled models (Claude Code keeps the `claude/cab/...` prefix). Strategy id aliases (`cab/…`, `cab-…`, legacy `claude-cab-auto`) normalize correctly in the route resolver.
- **Provider endpoint UI**: weight/priority fields are removed from the endpoint editor (unused in practice); protocol, URL, label, and enable remain.
- **Dark-theme toggle contrast**: off uses a zinc track and on uses green, so open/closed states are distinguishable when accent is white.

## [0.10.4] - 2026-08-11

### Fixed

- **Nameless Responses tools no longer break OpenAI-chat upstreams**: Codex sends Responses-style tool definitions, some of which (e.g. `external_web_access`) carry no `name`. When such a request fell back from the Responses endpoint to an OpenAI-chat endpoint, the nameless tool became `function.name: ""` and the upstream rejected the request with `Invalid 'tools[0].function.name': empty string` (400). Tool definitions without a name are now dropped during parsing, so protocol conversion never emits an empty function name.

## [0.10.3] - 2026-08-11

### Fixed

- **Sustained upstream outages no longer stall the gateway**: the HTTP client set only a total request timeout, so a provider whose connection hung would wait out the OS-level TCP timeout (~37s) before failing, then retry a second endpoint serially (~74s per request). Under concurrent/retried agent calls the gateway appeared hung. The client now has a 5-second `connect_timeout`, so unreachable upstreams fail fast.
- **Circuit breaker now skips unhealthy providers during fallback**: providers were never recorded as failed unless the _primary_ provider failed last, so a reseller whose endpoint kept failing couldn't be tripped, and `execute_with_fallback` would wait out the connect timeout on every request even after the breaker should have fired. Each attempted provider is now recorded individually, and the fallback loop skips providers the breaker has tripped — a sustained outage returns quickly instead of piling up. A tripped provider re-opens after a 30-second cooldown so it can recover after an upstream blip instead of staying broken until restart.

## [0.10.2] - 2026-08-10

### Fixed

- **Dashboard "Active models" under-counted reseller-served models**: the homepage tally only counted models whose native provider was enabled with a key, so a model served through an enabled endpoint on an active reseller provider (e.g. `deepseek/deepseek-v4-flash` via `opencode-go`) showed as 0 even though it was routable. The count now mirrors `/v1/models` routing — a model is active when enabled and reachable via its native provider OR an enabled endpoint whose provider is active — so the homepage and models page agree.
- **Streamed requests missing from usage stats**: streaming requests only wrote to `request_logs` and never inserted a `usage_records` row, so the usage page systematically under-counted input/output tokens and request counts for every streamed call (the page showed far fewer records than the request log). The gateway now records streamed requests in `usage_records` too, with the same cost calculation and cache-token layout handling as non-streamed requests.

## [0.10.1] - 2026-08-08

### Fixed

- **CI auto-publish no longer fails on tag pushes**: the `publish-release` job ran `gh release edit --draft=false` without an `actions/checkout` step, so `gh` had no git context and every release got stuck as a draft. The job now checks out the repository first, so releases auto-publish once all platform binaries are uploaded.

### Changed

- **Dashboard logo unified with the CAB favicon**: the sidebar used a generic stacked-cubes icon unrelated to the brand while the favicon and docs site use the sky-blue open-"C" mark. The sidebar now shows the same CAB mark, so every surface (browser tab, docs, dashboard) presents one consistent identity.
- **README rewritten and expanded**: new front page with version/platform/downloads/license/CI badges, a 2×2 screenshot grid of the live dashboard, grouped feature overview, quick-start steps, routing-strategy table, FAQ, and per-platform install instructions. Added a full **简体中文 (README.zh-CN.md)** edition mirroring the English structure. Screenshots refreshed to the current UI.

## [0.10.0] - 2026-08-08

### Added

- **Vision-capable model routing**: CAB now detects when a request embeds an image (Anthropic `image` blocks, OpenAI `image_url` parts, OpenAI Responses `input` items, Gemini `contents[].parts`, or a `data:image/` URI / image URL inline in plain text) and restricts routing to models that accept image input. Text-only models are excluded even when they are the cheapest option, so screenshot / diagram / UI-mockup requests no longer land on a model that rejects them (400) or produces garbage. Models.dev `architecture.modalities.input` data is authoritative; a slug-based fallback covers catalog entries without explicit modality data.

## [0.9.14] - 2026-08-07

### Added

- **OpenAI reasoning models via the Responses API**: models served through the native OpenAI surface (`@ai-sdk/openai`, e.g. GPT-5.6 Luna/Sol/Terra, o-series) now route via the OpenAI Responses protocol, which is the only API that properly handles reasoning effort, reasoning items, and tool-call interleaving. Anthropic `thinking` is converted to `reasoning.effort` (budget → low/medium/high/max); `@ai-sdk/openai-compatible` models (DeepSeek, vLLM…) stay on Chat Completions.
- **Manual-mode model pinning**: in manual mode, an agent's configured `model_id` now pins a specific model directly (resolved by name), instead of being treated as a routing strategy.

### Fixed

- **`cab update` self-replacement on Windows**: binary swap now tries a direct copy first, then renames the running binary to `cab.exe.old` and copies the new one in; the post-exit apply script kills leftover `cab.exe` processes via `taskkill /F` (excluding the updater itself), writes an apply log, and cleans up stale `.old` files.

## [0.9.13] - 2026-08-06

### Fixed

- **DeepSeek V4 / thinking-mode tool multi-turns via Claude Code**: OpenAI-chat upstreams (e.g. DeepSeek) require `reasoning_content` on every assistant tool-call turn when replaying history, but Claude Code strips unsigned thinking blocks so later turns arrive without it and upstream returns HTTP 400. The gateway now (1) emits a synthetic `signature` / `signature_delta` when converting OpenAI-compat reasoning into Anthropic thinking so Claude Code retains CoT across turns, (2) always maps thinking ↔ `reasoning_content` (empty string when stripped), and (3) injects missing `reasoning_content` on openai-chat tool-call history independent of cache shaping.

## [0.9.12] - 2026-08-05

### Fixed

- **`cab update` / `cab start` / `cab stop` stale-unit warning on Linux**: systemd printed `Warning: The unit file ... changed on disk. Run 'systemctl --user daemon-reload' to reload units.` whenever the `cab-srv.service` unit file was rewritten after the last reload (e.g. by a prior `cab service install` or an upgrade). The gateway now runs `systemctl [--user] daemon-reload` before every start/stop, so the warning can no longer appear.

## [0.9.11] - 2026-08-03

### Fixed

- **Request-log cache tokens double-counted**: some Anthropic-format relays (e.g. opencode.ai Console Go) report the total prompt as `usage.input_tokens` while also emitting `cache_read_input_tokens`, which is non-compliant with Anthropic's disjoint-input convention. The gateway now detects that layout and normalizes the stored legs so `total_tokens = non-cached input + cache read + output` instead of adding the cache read twice. This also fixes the per-request cost estimate, which previously billed the cached portion at full input rate.
- **DeepSeek cache hits not recorded for OpenAI-chat upstreams**: `usage.prompt_cache_hit_tokens` is now mapped to cache-read tokens, so cache hits show up in request logs instead of being counted as plain input.

## [0.9.10] - 2026-08-03

### Fixed

- **Claude Code 401 “Missing API key” through the gateway**: Anthropic-compatible upstreams that only accept the `x-api-key` header (e.g. opencode.ai Console Go) rejected requests authenticated solely with `Authorization: Bearer`. The gateway now forwards the provider key as `x-api-key` for `anthropic`-protocol upstreams, so Claude Code routes successfully again.

## [0.9.9] - 2026-08-03

### Fixed

- **macOS one-line install LaunchAgent noise / false failures**: `cab service install` no longer prints `launchctl` “Input/output error” for expected `bootout`/`unload` when nothing is loaded, and no longer `kickstart -k` right after `bootstrap` (which killed the fresh `RunAtLoad` process and forced extra restarts). `cab start` is idempotent when the agent is already loaded. `install.sh` / `install.ps1` no longer call a redundant `cab start` after `service install`.
- **`cab status` false “HTTP API unreachable” after install**: API probe timeout raised from 500ms to 2s so brief catalog-sync SQLite contention does not look like a dead daemon.

## [0.9.8] - 2026-07-31

### Fixed

- **Dashboard favicon**: ship `favicon.ico` / `favicon.svg` and link them from `app.html`, so `/favicon.ico` no longer falls back to `index.html`.

## [0.9.7] - 2026-07-31

### Changed

- **Upstream 429 handling uses exponential backoff before cooling a key**. Same key/endpoint retries up to 4 times (0.5s–8s, honors short `Retry-After`); only then marks a brief cooldown (default 60s, down from 1h) and falls back to the next key or model. Docs updated.

## [0.9.6] - 2026-07-31

### Changed

- **Linux release binaries are fully static (musl)**. Builds target `x86_64-unknown-linux-musl` / `aarch64-unknown-linux-musl`, so prebuilt packages no longer depend on host glibc (fixes `GLIBC_2.39 not found` on Deepin, Debian 12, Ubuntu 22.04, etc.). Install docs updated accordingly.

## [0.9.5] - 2026-07-30

### Changed

- **CI pipeline accelerated by ~50%**: Reduced from 4 runners to 3, split frontend build into a shared artifact job, switched Linux ARM64 from Docker-based `cross` to native `gcc-aarch64-linux-gnu` cross-compilation, added a `[profile.ci]` to Cargo.toml with LTO disabled. See [build-cli.yml](file:///c:/Users/ixion/workspace/cab/.github/workflows/build-cli.yml) and [Cargo.toml](file:///c:/Users/ixion/workspace/cab/Cargo.toml).

## [0.9.4] - 2026-07-30

### Fixed

- **`cab service install` no longer fails when the scheduled task already exists with elevated ACLs**: `install_user()` now tries to delete the existing task before re-creating it, and if both delete and create fail with "Access denied" (e.g. the task was created in a previous elevated session), it falls back to a warning instead of failing the whole install. The updated launcher files (VBS/CMD) are already in place, so the existing task will use them on next start. This fixes the `错误: 拒绝访问` error during `irm | iex` re-install on Windows.

## [0.9.3] - 2026-07-30

### Fixed

- **PowerShell installer file-lock on upgrade**: `install.ps1` (and `install.sh`) now stop any running CAB service (`schtasks /End`, `taskkill /IM cab.exe`) before replacing `cab.exe`, and fall back to `cmd /c copy /Y` if `Copy-Item` still fails. This resolves "The process cannot access the file because it is being used by another process" when re-running the one-line installer with CAB already installed and running.

## [0.9.2] - 2026-07-30

### Fixed

- **`cab update` self-termination on Windows**: `stop_user()` no longer kills the running `cab.exe` process. `taskkill` now excludes the current PID via `/FI "PID ne {self_pid}"`, so `cab update` survives to complete the swap.
- **Windows binary replacement during update**: `install_file()` now falls back to `cmd /c copy /Y` (different sharing semantics than `fs::rename`/`fs::copy`) when the destination is locked, and `run_update()` schedules a post-exit batch script (via `wscript`) to stage + replace the running executable and restart the service after the updater exits. This resolves "另一个程序正在使用此文件" errors when self-updating on Windows.

## [0.9.1] - 2026-07-30

### Added

- **Windows install script**: `scripts/install.ps1` (and docs mirror) — one-line `irm … | iex` install for x64/ARM64, PATH + user service setup.

### Changed

- **README / install docs**: document PowerShell installer alongside `install.sh`.

## [0.9.0] - 2026-07-30

### Changed

- **Single `cab` binary (hard cut)**: ships only `cab` — no `cab-cli` / `cab-srv` / `cab-gui` aliases or desktop installers. Bare `cab` prints help; run `cab serve` for the foreground daemon, `cab gui` to start the gateway if needed and open the browser dashboard.
- **Service units** launch `{install_dir}/cab serve` (Windows SCM still uses `cab --service`).
- **Install / update** operate on one binary + `ui/` (`scripts/install.sh`, `cab update`, CLI release archives).
- **Workspace**: `cab-srv` is library-only; Tauri (`src-tauri`) removed; release CI builds CLI archives without WebKit.

### Migration from 0.8.x

1. Uninstall the old service: `cab-cli service uninstall` (or stop/delete the `cab-srv` unit / task).
2. Reinstall with the curl installer (or replace `~/.cab/bin` with the new archive).
3. Use `cab …` thereafter; open the UI with `cab gui`. Desktop `.dmg` / `.msi` / AppImage packages are no longer shipped.

## [0.8.7] - 2026-07-30

### Added

- **One-line CLI install**: `curl -fsSL https://raw.githubusercontent.com/xiongdi/cab/main/scripts/install.sh | bash` installs `cab-cli` + `cab-srv` (+ UI) into `~/.cab/bin`. Release CI packages `cab-<os>-<arch>.tar.gz` / `.zip` assets; docs mirror at `https://xiongdi.github.io/cab/install.sh`.
- **`cab-cli update`**: download the latest (or `--version`) CLI release archive, replace binaries/UI, and restart the service (`--check` to only report).

### Changed

- **Normalized token storage**: request logs / usage records store `input_tokens` without cache **read**; `cache_read_tokens` and `cache_creation_tokens` (write) are separate. OpenAI wire `prompt`/`input` still includes reads — only the read portion is stripped on ingest; write remains a billing overlay. Anthropic keeps the three prompt legs disjoint.
- **Frontend cache hit rate**: token-weighted `cache_read / prompt`, protocol-aware display input, two decimal places (e.g. `99.31%`). Usage summary input totals include cache legs for display.

### Fixed

- **`cab-srv` preferred local `build/` UI** over packaged `/usr/share/cab/ui`, so refreshing `:3125` during development no longer serves a stale system UI.
- **Artificial Analysis catalog status**: persist AA JSON cache on sync and prefer SQLite for `synced_at`; AA sync still runs when models.dev download fails.
- **Upstream usage parsing**: OpenAI Chat prefers `prompt_tokens`/`completion_tokens` over zeroed LongCat aliases; Responses reads nested `response.usage`; cache details from official `*_tokens_details` fields.

## [0.8.6] - 2026-07-29

### Changed

- **Supported coding agents narrowed to four**: Claude Code, Codex, OpenCode, and Grok Build. Hermes, Kilo Code, OpenClaw, Pi, and Reasonix integrations are removed; existing installs drop those agents on upgrade and pick up Grok Build via seed merge.
- **Grok Build integration**: Auto/Manual modes rewrite `~/.grok/config.toml` with `cab-*` OpenAI chat-completions models pointing at the local gateway; Native mode restores the previous default.

### Fixed

- **TOML config parsing for Codex / Grok Build**. Prefer `toml::from_str` over `str::parse` so existing `config.toml` tables are not silently discarded under toml 1.x.

## [0.8.5] - 2026-07-28

### Fixed

- **Cache hit rate displayed incorrectly on dashboard**. For Anthropic-style API responses, `input_tokens` excludes cached tokens — `total_tokens` was computed as `input_tokens + output_tokens`, causing the token-ratio bar and cache hit percentage to show inflated values (sometimes >100%). Now detects Anthropic-style cache keys and includes `cache_read_tokens` + `cache_creation_tokens` in the total.

## [0.8.4] - 2026-07-28

### Added

- **Agent-aware model listing**. `GET /v1/models` now returns a single `claude-cab-auto` stub when called by Claude Code in `auto` mode — the in-CLI model picker shows one entry instead of hundreds, and CAB auto-routes the request regardless of the chosen model. For all other agents/modes, returns the de-duplicated list of available models (no more discovery-alias or short-suffix duplicates).

### Config

- **AA model map updated**. Added `opus-5` and `sonnet-5` entries for catalog sync.

## [0.8.3] - 2026-07-27

### Fixed

- **Windows user-scope scheduled task no longer flashes a CMD window**. Switched the Task Scheduler `ONLOGON` launcher to a hidden VBS wrapper (`wscript.exe` → `.vbs` → `.cmd`), so console `cab-srv` starts invisibly. Also hardened `cab-cli`/`cab-srv` path discovery across NSIS/MSI/`_up_`/resources layouts, defaulted the service-scope prompt to the current user, and added a plain `/TR` fallback when the `/XML` task-create path is denied by prior elevated-task ACLs.
- **CI race between `cab-core` env tests**. `paths::cab_home_respects_env` and the benchmark catalog cache test both mutate `HOME`/`CAB_HOME` and now share a single `ENV_TEST_LOCK`, preventing intermittent failures when cargo runs lib tests in parallel.

## [0.8.2] - 2026-07-21

### Fixed

- **Dashboard i18n gaps**. Usage page subtitle and other hardcoded UI strings (settings, logs, agents, models, dashboard) now use `translations.ts` in both zh/en.
- **Light/dark theme contrast**. Replaced hardcoded `rgba(255,…)` overlays and fixed hex colors with theme-aware CSS variables across all pages and shared components.
- **Linux white screen (cab-gui)**. Set `WEBKIT_DISABLE_DMABUF_RENDERER=1` on startup for WebKitGTK setups where DMABUF/GBM fails (e.g. Deepin, some NVIDIA drivers).

## [0.8.1] - 2026-07-21

### Fixed

- **Linux release builds on Ubuntu 22.04** (`ubuntu-22.04` / `ubuntu-22.04-arm`). Packages no longer link against glibc 2.39 from Ubuntu 24.04, so they run on glibc 2.35+ hosts (e.g. Deepin 25, Ubuntu 22.04, Debian 12).

### Changed

- **Desktop OS minimums aligned with Tauri 2**: Windows 7+, macOS 10.15 Catalina+ (`bundle.macOS.minimumSystemVersion`), Linux WebKitGTK 4.1 + glibc 2.35+.

## [0.8.0] - 2026-07-21

### Added

- **Dual-scope `cab-srv` service install (`user` | `system`)**. Choose at install / first GUI launch:
  - **user** (default): data in `~/.cab`; Linux `systemd --user` + linger; macOS LaunchAgent; Windows Task Scheduler (ONLOGON, restart-on-failure).
  - **system**: data in `/var/lib/cab`, `/Library/Application Support/cab`, or `%ProgramData%\cab`; requires elevation. Linux systemd as user `cab` with hardening; macOS LaunchDaemon as `_cab` when creatable; Windows SCM service as `LocalService` with service-scoped `Environment` (not machine-wide `setx`).
- **`CAB_HOME` data-directory override**. Runtime data root follows `CAB_HOME`, then scope defaults.
- **Windows SCM entry** (`cab-srv --service`) for system-scope installs via the `windows-service` crate.
- **NSIS installer scope prompt** and **cab-gui first-run scope dialog** (system path self-elevates via UAC / `pkexec` / `osascript`).

### Changed

- **`cab-gui` is a thin client**. The desktop shell no longer embeds the gateway; it ensures `cab-srv` is running and opens `http://127.0.0.1:{port}/`. Closing the GUI leaves the daemon running.
- **`cab-srv` serves the static dashboard** (`CAB_FRONTEND_DIR`, bundled `ui/`, or `/usr/share/cab/ui`).
- **Removed Windows Startup `.bat`** approach in favor of Task Scheduler / SCM.

### Documentation

- Install, gateway-auth, and architecture guides (EN/ZH) document scopes, paths, elevation, and platform mechanisms.

## [0.7.1] - 2026-07-17

### Fixed

- **CI clippy failures on `main`**. Resolved a `clippy::question_mark` lint (new in clippy 1.97) in `crates/cab-gateway/src/protocol/ir.rs` that flagged the image-source parsing. Rewrote the `media_type` / `source` extraction as explicit `if let` / `match` chains. Also corrected the release-notes template (`scripts/generate-release-body.sh`), which still claimed a universal `.dmg` after the v0.7.0 macOS split.

## [0.7.0] - 2026-07-17

### Added

- **Dashboard redesign**. Overhauled the Svelte dashboard layout and component styling, including refined light/dark theme support driven by CSS variables.
- **Log detail expansion**. Expanded the request-log detail view for richer inspection of gateway traffic.
- **Auto-install daemon on first launch + CLI in PATH**. CAB now installs the `cab-srv` daemon automatically on first launch and puts the `cab` CLI on the system `PATH`, so the gateway is running and the command is reachable without manual setup.
- **GOAL.md**. New top-level project goals and quickstart documentation.

### Changed

- **Cache token IR fix**. Corrected how cache-read/creation tokens are captured through the gateway's intermediate representation, so cache-hit costing and reporting stay accurate.
- **Artificial Analysis data migrated from JSON file to SQLite**. AA benchmark data now lives in `~/.cab/cab.db` alongside the rest of the catalog, removing the standalone JSON cache file.

### Build

- **Split macOS builds into separate Intel and Apple Silicon packages**. macOS releases now ship distinct `x64` (Intel) and `arm64` (Apple Silicon) `.dmg` assets instead of a single universal binary.

### Refactored

- **Unified package naming to `cab-cli` / `cab-srv` / `cab-gui`**. Rust crate and package names consolidated for a consistent identity across the CLI, daemon, and desktop GUI.

### Fixed

- **Clippy errors + dynamic log retention test**. Resolved compiler clippy warnings across the workspace and fixed the dynamic log-retention test.

## [0.6.1] - 2026-07-05

### Changed

- **Unified upstream auth to `Authorization: Bearer`**. Removed the `x-api-key` header for Anthropic-protocol endpoints. Both official Anthropic and OpenAI APIs accept Bearer authentication, and third-party proxies (LongCat, etc.) universally support it. The gateway no longer discriminates auth headers by protocol.

### Fixed

- **Catalog sync `synced_at` timestamps stuck at first sync**. The models.dev and Artificial Analysis sync functions downloaded and processed catalog data entirely in memory without writing back to the disk cache. The `synced_at` field (derived from file mtime or embedded timestamp) never advanced beyond the initial write. Both sync paths now persist the downloaded data to `~/.cab/catalog/` after each successful sync, fixing the stuck timestamps and providing an offline cache for subsequent startups.

## [0.6.0] - 2026-07-05

### Added

- **`cab-cli` CLI + `cab-srv` daemon**. New cross-platform service controls for managing the CAB gateway as a background daemon. On macOS: LaunchAgent plist (`~/Library/LaunchAgents/com.cab.cab-srv.plist`) with `launchctl` integration. On Windows: Windows Service via `cab-srv start/stop/restart/service`. On Linux: systemd user service (`~/.config/systemd/user/cab-srv.service`). The `cab-cli` CLI wraps all daemon lifecycle commands.
- **Homebrew & WinGet packaging manifests**. CAB is now installable via `brew tap xiongdi/cab && brew install cab` (macOS/Linux) and WinGet (Windows). See `homebrew/cab.rb` and `winget/` in the repo root for source.
- **Docker deployment support**. New `Dockerfile` and `docker-compose.yml` for containerized headless gateway deployment.

### Changed

- **License updated to Auditable Commercial License (ACL) v1.0** — a commercial source-available license with audit rights for licensees. See `LICENSE` for full terms.

### Fixed

- **CI clippy + formatting warnings resolved**. All workspace targets now pass `cargo clippy -- -D warnings` and `cargo fmt --check` cleanly.

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

- Local UAT now starts the **release** `cab-srv` binary and invokes **real coding-agent CLIs** (UAT-10/11/12).
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
- `cab-srv` library surface and expanded integration / system test coverage.

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
