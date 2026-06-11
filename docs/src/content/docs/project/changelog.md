---
title: Changelog
description: Release history for CAB.
---

CAB follows [Semantic Versioning](https://semver.org/). Full changelog: [CHANGELOG.md on GitHub](https://github.com/xiongdi/cab/blob/main/CHANGELOG.md).

## v0.2.3

- **Codex**: dynamic auth via `auth.json` (ChatGPT OAuth) in CAB-managed modes
- **Codex**: backup and restore of existing OpenAI/ChatGPT credentials when toggling modes

## v0.2.2

- Node.js 24+, Rust stable toolchain, `toml` 1.x
- Dependency and CI workflow updates

## v0.2.1

- Real coding-agent CLI integration tests (UAT)
- Four routing strategies × seven agents verified end-to-end

## v0.2.0

- Persistent `~/.cab/state.json` for agents and routes
- Gateway Bearer auth (`gateway_key`, `auth_enabled`)
- New `cab-services` application layer
- JSONL request logs with retention
- `POST /api/routing/explain` and routing preview UI
- Plugin/adapter refactor for agents and protocols

## v0.1.x

- Initial release: local LLM gateway for coding agents
- Seven agent integrations
- Bilingual desktop UI (EN / 简体中文)
- Windows, macOS, Linux installers

Download the latest release on [GitHub Releases](https://github.com/xiongdi/cab/releases).
