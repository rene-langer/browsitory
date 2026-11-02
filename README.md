<p align="center">
  <img src="crates/tauri-app/icons/128x128@2x.png" width="96" alt="Browsitory icon">
</p>

<h1 align="center">Browsitory</h1>

<p align="center">A fast, keyboard-driven Git desktop client.</p>

<p align="center">Open several repositories at once as tabs, and save recurring multi-repo projects as <strong>workspaces</strong> — a folder of repos you open, group, and close together in one action.</p>

## VSCode extension

Browsitory also runs inside VSCode (1.134 or newer; Linux x64, macOS, Windows x64). It is a
pre-release for now:

- **Visual Studio Marketplace**: search for "Browsitory" in the Extensions view and pick
  **Install Pre-Release Version**, or run
  `code --install-extension browsitory.browsitory --pre-release`.
- **Open VSX** (VSCodium, Cursor and others): search for "Browsitory" there, or see
  <https://open-vsx.org/extension/browsitory/browsitory>.
- **From a file**: download the `.vsix` for your platform from the
  [releases page](https://github.com/rene-langer/browsitory/releases) and run
  `code --install-extension browsitory-<version>-<platform>.vsix`.

Then run **Browsitory: Open** from the Command Palette. See the
[User Guide](docs/USER_GUIDE.md#installing-the-vscode-extension).

## Quickstart

```bash
git clone <this-repo-url>
cd browsitory
cd frontend && pnpm install && cd ../crates/tauri-app
cargo tauri dev
```

Requires Rust, [pnpm](https://pnpm.io), the [Tauri CLI](https://tauri.app/) (`cargo install tauri-cli`), and the Tauri prerequisites for your OS.

## Building a distributable

```bash
scripts/build-dist.sh      # Linux/macOS: .deb/.rpm/.AppImage or .dmg
scripts\build-dist.ps1     # Windows: .msi/.exe
```

Bundles land in `crates/tauri-app/target/release/bundle/`.

See the [User Guide](docs/USER_GUIDE.md) for a tour of the UI, and [CLAUDE.md](CLAUDE.md) / [AGENTS.md](AGENTS.md) for full build, test, and E2E commands.
