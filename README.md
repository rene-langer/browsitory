# Browsitory

A fast, native desktop Git client, inspired by the speed and keyboard-driven workflow of tools
like Sublime Merge. Written in Rust, with a custom-drawn UI (`egui`) and libgit2 (`git2`) for
git operations — no browser, no server, direct filesystem access to your repositories.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

> **Status: Phase 1, 2 & 3 complete**. Implemented today: opening a local repository, commit
> history, file diffs, staging/unstaging, committing, branch management, stash, merge with
> conflict resolution, interactive rebase (pick/reword/edit/squash/fixup/drop), a blame viewer,
> a multi-branch commit graph, and remote operations (push/pull/fetch with progress reporting,
> multi-remote support, tag push, non-interactive SSH-agent/credential-manager auth). Worktrees,
> submodules, reflog, and PR integration are on the [roadmap](#roadmap) but not yet implemented.
> See [Project Setup](docs/PROJECT_SETUP.md) for the full phase plan.

## Features

- 🖥️ **Native desktop app** — Windows, macOS, and Linux; no browser required
- 🔍 **Visual diff viewer** — word-level highlighting on changed lines
- 📝 **Staging area** — stage/unstage files, write commit messages, commit
- 📜 **Commit history** — paginated, lazy-loaded commit log
- 🌿 **Branch management, stash, merge, rebase, blame, commit graph** — implemented
- 🌐 **Remote operations** — push, pull, fetch, tag push, and multi-remote management, all with
  progress reporting and non-interactive SSH-agent/credential-manager authentication
- ⚡ **Fast** — direct libgit2 bindings, no JavaScript git reimplementation

## Quick Start

```bash
# One-time setup (installs Rust + system build deps): see scripts/setup-dev.sh
./scripts/setup-dev.sh

# Build and run
cargo build --workspace
cargo run -p app
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full development workflow.

## Tech Stack

| Layer | Technology | License |
|-------|-----------|---------|
| GUI | [egui](https://github.com/emilk/egui) / [eframe](https://github.com/emilk/egui) | MIT/Apache-2.0 |
| Git operations | [git2](https://github.com/rust-lang/git2-rs) (libgit2 bindings) | MIT/Apache-2.0 binding; libgit2 itself is GPL-2.0-with-linking-exception — see [License Compliance](docs/LICENSE_COMPLIANCE.md) |
| Diffing | [similar](https://github.com/mitsuhiko/similar) | MIT/Apache-2.0 |
| Folder picker | [rfd](https://github.com/PolyMeilex/rfd) | MIT/Apache-2.0 |
| Config/preferences | serde + toml + directories | MIT/Apache-2.0 |
| Tests | built-in `cargo test`, real temp-dir git repos | — |

## Project Structure

```
browsitory/
├── crates/
│   ├── git-core/   # git2-based service layer (status, log, diff, stage, commit, ...)
│   ├── config/     # repo registry + preferences (TOML, OS config dir)
│   └── app/        # egui/eframe desktop UI
├── docs/
│   ├── FEATURES.md
│   ├── ARCHITECTURE.md
│   └── PROJECT_SETUP.md
├── scripts/
│   └── setup-dev.sh
└── Cargo.toml      # workspace manifest
```

## Documentation

- **[Features](docs/FEATURES.md)** — complete feature specifications
- **[Architecture](docs/ARCHITECTURE.md)** — system design and technology choices
- **[Project Setup](docs/PROJECT_SETUP.md)** — workspace layout and phase roadmap
- **[Development Guide](docs/DEVELOPMENT.md)** — development setup and conventions
- **[License Compliance](docs/LICENSE_COMPLIANCE.md)** — dependency license audit

## Contributing

Contributions are welcome — see [CONTRIBUTING.md](CONTRIBUTING.md).

## License

This project is licensed under the MIT License — see the [LICENSE](LICENSE) file for details.
One dependency (`git2`, via vendored libgit2) is GPL-2.0-with-linking-exception rather than
MIT; see [License Compliance](docs/LICENSE_COMPLIANCE.md) for why that's still fine.

## Roadmap

- [x] Cargo workspace scaffold (git-core / config / app crates)
- [x] Phase 1: open repo, status, commit history, diff viewer, staging, commit
- [x] Phase 2: branch management, stash, merge with conflict resolution, interactive rebase
      (with reword/squash — a capability gain over the old browser build), blame, commit graph
- [x] Phase 3: remote operations (push/pull/fetch, credentials, multi-remote support, tag push)
      — reachable directly thanks to libgit2's native transport support, no server backend needed
- [ ] Phase 4: worktrees, submodules, reflog, PR integration

## Support

For issues, feature requests, or questions, please open an issue on GitHub.

---

**Status**: Phase 1, 2 & 3 complete — breaking changes expected. Not ready for production use yet.
