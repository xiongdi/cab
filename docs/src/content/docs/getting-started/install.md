---
title: Install
description: Download and install CAB for Windows, macOS, and Linux.
---

## One-line install

Install the single `cab` binary (+ dashboard UI) from GitHub Releases:

```bash
curl -fsSL https://raw.githubusercontent.com/xiongdi/cab/main/scripts/install.sh | bash
# mirror: curl -fsSL https://xiongdi.github.io/cab/install.sh | bash
```

This installs into `~/.cab/bin`, adds it to your shell `PATH`, and runs `cab service install --scope user`.

```bash
cab status
cab gui                     # open the dashboard in your browser
cab update                  # upgrade to the latest release
cab update --check          # only check for a newer version
cab update --version 0.9.0
```

Options for the installer:

```bash
curl -fsSL …/install.sh | bash -s -- --version 0.9.0
curl -fsSL …/install.sh | bash -s -- --no-service --no-modify-path
```

Desktop `.dmg` / `.msi` / AppImage packages are **not** shipped (0.9+). Use the CLI and open the UI with `cab gui`. To build from source, see the [repository README](https://github.com/xiongdi/cab#getting-started).

## System requirements

| Platform    | Minimum version | Architectures                         | Notes                                     |
| ----------- | --------------- | ------------------------------------- | ----------------------------------------- |
| **Windows** | Windows 7+      | x64, ARM64                            | No WebView required; dashboard in browser |
| **macOS**   | 10.15 Catalina+ | Intel (x86_64), Apple Silicon (arm64) | Separate archive per architecture         |
| **Linux**   | glibc 2.35+     | x64, ARM64                            | Built on Ubuntu 22.04; e.g. Ubuntu 22.04+ |

Build from source with `cargo run -p cab --bin cab -- serve` for release testing, or use a pre-built archive from GitHub Releases. For daily development, follow the two-terminal workflow in [AGENTS.md](https://github.com/xiongdi/cab/blob/main/AGENTS.md).

## Release archives

Replace `VERSION` with the release number without the `v` prefix (e.g. `0.9.0`). Assets on each release:

| Platform                    | File                                                |
| --------------------------- | --------------------------------------------------- |
| Linux x64 / ARM64           | `cab-linux-x64.tar.gz` / `cab-linux-arm64.tar.gz`   |
| macOS Intel / Apple Silicon | `cab-darwin-x64.tar.gz` / `cab-darwin-arm64.tar.gz` |
| Windows x64 / ARM64         | `cab-windows-x64.zip` / `cab-windows-arm64.zip`     |

Each archive contains `cab` (or `cab.exe`) and an `ui/` directory.

## After install

1. Run `cab gui` (starts the service if needed and opens the dashboard).
2. If you skipped service install, choose **service scope**:
   - **Current user** (default) — data in `~/.cab`; starts after login.
   - **System** — data in `/var/lib/cab`, `/Library/Application Support/cab`, or `%ProgramData%\cab`; needs admin/root; starts at boot.
3. Continue with the [Quick Start](../quick-start/) guide.

```bash
cab service install --scope user   # or: --scope system (elevated)
cab start
cab status
cab gui
```

## Troubleshooting

| Symptom             | Fix                                                                               |
| ------------------- | --------------------------------------------------------------------------------- |
| Browser won't open  | Visit `http://127.0.0.1:3125/` manually                                           |
| Agent can't connect | Ensure CAB is running; port `3125` is free                                        |
| Upgrading from 0.8  | Uninstall old `cab-cli`/`cab-srv` service, then reinstall with the curl installer |
