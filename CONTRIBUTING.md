# Contributing to CAB

Thank you for your interest in contributing to CAB (Coding Agents Bridge)! We welcome bug reports, feature suggestions, and pull requests.

## Development Setup

To build and run CAB locally, you need the following prerequisites installed on your system:

1. **Rust Toolchain**: Install via [rustup](https://rustup.rs/) (edition 2024; repo `rust-toolchain.toml` pins `stable`).
2. **Node.js & npm**: Install Node.js (v24 or higher, LTS).
3. **OS-specific dependencies for Tauri**: Follow the [Tauri 2 Prerequisites Guide](https://v2.tauri.app/start/prerequisites/) (e.g., on Linux, you will need `libwebkit2gtk-4.1` and other system packages).
4. **OpenSSL**: Required by the server component to dynamically generate local SSL certificates.

### Build and Run Steps

1. **Clone the repository**:

   ```bash
   git clone git@github.com:xiongdi/cab.git
   cd cab
   ```

2. **Install frontend dependencies**:

   ```bash
   npm install
   ```

3. **Run in development mode** (daily dev — see [AGENTS.md](AGENTS.md) for full rules):

   Two terminals, two processes, globally unique ports (3125 backend, 5173 frontend):

   ```bash
   # Terminal A — backend (cargo watch, port 3125)
   npm run dev:server

   # Terminal B — frontend (Vite hot reload, port 5173)
   npm run dev
   ```

   > Do **not** use `cargo run -p cab-srv`, `npm run tauri:dev`, or `npm run dev:server:once` for daily dev — they conflict with the watch server. See AGENTS.md for the full forbidden list and port-conflict resolution.

4. **Desktop GUI / release testing** (separate from daily dev):
   - **Tauri desktop app** (for packaging and UI testing): `npm run tauri:dev`
   - **Headless release binary** (for UAT / production): `cargo run -p cab-srv`

5. **Binding to privileged ports (e.g., port 443 for DNS hijacking proxy)**:
   CAB needs `cap_net_bind_service` permission to run on port 443 without root on Linux. You can use the provided helper script:
   ```bash
   ./scripts/run-with-setcap.sh
   ```

## Development Workflow

### Code Quality and Guidelines

- **Rust Formatting**: We use standard Rust formatting. Check with:

  ```bash
  cargo fmt --all -- --check
  ```

  Fix formatting automatically with:

  ```bash
  cargo fmt --all
  ```

- **Rust Linting**: Run Clippy to catch common errors and patterns:

  ```bash
  cargo clippy --workspace --all-targets -- -D warnings
  ```

- **Frontend Checking**: Ensure TypeScript and Svelte code compiles correctly:

  ```bash
  npm run check
  ```

- **Testing**: Run Rust tests to ensure no regressions:
  ```bash
  cargo test --workspace
  ```

### Submitting a Pull Request

1. Fork the repository and create your branch from `main`.
2. Ensure all tests and lint checks pass.
3. Write clean commit messages.
4. Open a pull request against the `main` branch.
