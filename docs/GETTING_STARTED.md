# Getting Started with Browsitory

Welcome to Browsitory! This guide will help you set up and understand the project.

## What is Browsitory?

Browsitory is a native, cross-platform desktop Git client (Windows/macOS/Linux) built in Rust,
with a fast, custom-drawn UI inspired by Sublime Merge. It provides:
- Commit history viewing
- Diff viewing (with word-level highlighting)
- Staging and committing changes
- Branch management, stash, merge, rebase, blame, and a commit graph (planned — see
  [PROJECT_SETUP.md](PROJECT_SETUP.md) for what's implemented today vs. on the roadmap)

Unlike the project's original browser-based prototype, this is a plain native binary: it opens
repositories directly from the local filesystem via libgit2, with no browser sandbox, no
service worker, and no server.

## Quick Start (5 minutes)

### 1. One-time host setup
```bash
cd browsitory
./scripts/setup-dev.sh
```
Installs Rust (via rustup, if not already present) and the system packages `eframe`/`winit`
and `git2`'s vendored libgit2 build need, then verifies the workspace builds.

### 2. Run the app
```bash
cargo run -p app
```

### 3. Open a repository
Click "Open Repository..." in the app and pick any local git repository.

## Project Structure

```
browsitory/
├── docs/                    # Documentation
│   ├── FEATURES.md         # Complete feature list
│   ├── ARCHITECTURE.md     # Tech stack & design
│   ├── DEVELOPMENT.md      # Development guide
│   └── LICENSE_COMPLIANCE.md
├── crates/
│   ├── git-core/           # git2-based service layer
│   ├── config/             # repo registry + preferences
│   └── app/                # egui/eframe desktop UI
├── scripts/setup-dev.sh    # dev host setup
├── README.md
└── Cargo.toml
```

## Tech Stack at a Glance

- **egui / eframe** — immediate-mode Rust GUI
- **git2** — libgit2 bindings for all git operations
- **similar** — line + word-level diffing
- **rfd** — native folder-picker dialog
- **serde + toml + directories** — config/preferences persistence

## Key Concepts

### 1. Native desktop app, not a web app
No browser, no install-to-home-screen flow, no offline caching to reason about — the app has
direct, unrestricted filesystem access the moment it opens a repository.

### 2. git-core is dependency-injected
Every git operation is a plain function taking a `&git2::Repository` explicitly. This is what
makes the whole service layer testable without a GUI — see `crates/git-core/tests/`.

### 3. One worker thread per open repository
Git operations run on a background thread (one per open repo) so the UI never blocks; see
`docs/ARCHITECTURE.md`'s "Threading model" section.

## Common Tasks

### View Commit History
1. Open a repository
2. The commit history panel at the bottom lists commits newest-first

### Stage Changes
1. In the "Changes" panel, click "+" next to an unstaged file to stage it
2. Click "-" next to a staged file to unstage it

### Create a Commit
1. Stage the files you want to commit
2. Write a commit message
3. Click "Commit"

## Development Workflow

```bash
git checkout -b feature/my-feature
# make changes
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
git add .
git commit -m "feat: add new feature"
git push origin feature/my-feature
```

## Next Steps

### For Users
1. Open a local git repository
2. Explore the commit history and diff viewer
3. Try staging changes and committing

### For Contributors
1. Read [DEVELOPMENT.md](DEVELOPMENT.md)
2. Read [CONTRIBUTING.md](../CONTRIBUTING.md)
3. Pick a Phase 2 item from [PROJECT_SETUP.md](PROJECT_SETUP.md) or an open issue

## Troubleshooting

### Build fails with missing system libraries
Run `scripts/setup-dev.sh`, or see [DEVELOPMENT.md](DEVELOPMENT.md#troubleshooting) for the
exact package names per OS.

### First build is very slow
Expected — `git2` compiles vendored libgit2 from source, and `eframe`'s dependency tree
(`wgpu`/`winit`/`egui`) is large. Later builds are incremental.

## Resources

- **[Documentation Index](../README.md)**
- **[Features](FEATURES.md)**
- **[Architecture](ARCHITECTURE.md)**
- **[Development Guide](DEVELOPMENT.md)**
- **[Contributing](../CONTRIBUTING.md)**

## License

Browsitory is licensed under the MIT License — see [LICENSE](../LICENSE) for details, and
[LICENSE_COMPLIANCE.md](LICENSE_COMPLIANCE.md) for the one documented dependency exception.

---

**Ready to get started?** Run `./scripts/setup-dev.sh && cargo run -p app`!
