<p align="center">
  <img src="crates/tauri-app/icons/128x128@2x.png" width="96" alt="Browsitory icon">
</p>

<h1 align="center">Browsitory</h1>

<p align="center">A fast, keyboard-driven Git desktop client.</p>

## Quickstart

```bash
git clone <this-repo-url>
cd browsitory
cd frontend && pnpm install && cd ../crates/tauri-app
cargo tauri dev
```

Requires Rust, [pnpm](https://pnpm.io), the [Tauri CLI](https://tauri.app/) (`cargo install tauri-cli`), and the Tauri prerequisites for your OS.

See [CLAUDE.md](CLAUDE.md) / [AGENTS.md](AGENTS.md) for full build, test, and E2E commands.
