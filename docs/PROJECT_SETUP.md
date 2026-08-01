# Browsitory - Project Setup Summary

## Project Overview

**Browsitory** is a native, cross-platform desktop Git client written in Rust, with a
Sublime-Merge-inspired UI built on `egui`. It was originally built as a browser PWA (see git
history before branch `feat/rust_from_scratch`); that codebase was deleted and rewritten from
scratch rather than ported incrementally, once it became clear a native app was a better fit
for the product goals (direct filesystem access, no browser sandbox limitations, full libgit2
capability instead of a partial pure-JS reimplementation).

### Name: **Browsitory**
"Browser" + "Repository" — a name coined back when this was a browser-based tool. Kept as the
project name since the rewrite; it no longer describes the delivery mechanism, just the brand.

---

## Project Structure

```
browsitory/
├── docs/
│   ├── FEATURES.md              # Complete feature specifications
│   ├── ARCHITECTURE.md          # Tech stack & system design
│   ├── DEVELOPMENT.md           # Development guide & patterns
│   ├── GETTING_STARTED.md       # Quick start guide
│   ├── LICENSE_COMPLIANCE.md    # License verification
│   └── PROJECT_SETUP.md         # This file
│
├── crates/
│   ├── git-core/                 # git2-based service layer
│   │   ├── src/                  # repo, status, log, diff, stage, commit, branch, stash,
│   │   │                          # merge, rebase, conflict, blame, graph, remote,
│   │   │                          # credentials, transfer (push/pull/fetch)
│   │   └── tests/                # integration tests against real temp-dir repos
│   ├── config/                   # repo registry + preferences (TOML)
│   │   └── src/
│   └── app/                      # egui/eframe desktop UI
│       └── src/
│           ├── worker.rs         # per-repo worker thread + command/event protocol
│           ├── state.rs          # AppState, RepoSession
│           ├── ui/                # one module per panel
│           ├── theme.rs
│           └── main.rs
│
├── scripts/
│   └── setup-dev.sh              # one-shot dev host setup (Rust + system build deps)
│
├── Cargo.toml                    # workspace manifest
├── rust-toolchain.toml           # pinned stable channel
├── README.md
├── LICENSE                       # MIT
└── CONTRIBUTING.md
```

---

## Technology Stack

| Layer | Technology | License | Purpose |
|-------|-----------|---------|---------|
| GUI | egui / eframe | MIT/Apache-2.0 | Immediate-mode, custom-drawn desktop UI |
| Git operations | git2 (libgit2 bindings) | MIT/Apache-2.0 binding; libgit2 is GPL-2.0-with-linking-exception | All git operations |
| Diffing | similar | MIT/Apache-2.0 | Line + word-level diffing |
| Folder picker | rfd | MIT/Apache-2.0 | Native "Open Repository" dialog |
| Config/preferences | serde, toml, directories | MIT/Apache-2.0 | Repo registry + preferences persistence |
| Error handling | thiserror | MIT/Apache-2.0 | Typed error enums per crate |
| Tests (dev-only) | tempfile | MIT/Apache-2.0 | Real temp-dir repos/config files for tests |

See [LICENSE_COMPLIANCE.md](LICENSE_COMPLIANCE.md) for the full dependency audit, including the
deliberate libgit2 license exception.

---

## Cargo commands

```bash
cargo build --workspace              # build all crates
cargo run -p app                     # launch the desktop app
cargo test --workspace                # run all tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

First-time host setup: `scripts/setup-dev.sh` (installs Rust via rustup if missing, plus the
system packages `eframe`/`winit` and `git2`'s vendored libgit2 build need).

---

## Development Roadmap

### Phase 1: Native MVP (current)
- [x] Cargo workspace scaffold (git-core / config / app crates)
- [x] Repository opening (direct filesystem access via `git2::Repository::discover`)
- [x] Status (staged/unstaged file list)
- [x] Commit history (paginated `Revwalk`)
- [x] Diff viewer (line-level + word-level highlighting on changed lines)
- [x] Staging/unstaging + commit creation
- [x] Unit/integration test tooling (`cargo test`) + CI matrix (Linux/macOS/Windows)

#### Phase 1 scope cuts (deliberate)
- No branch switching, stash, merge, rebase, blame, or commit graph yet — see Phase 2.
- No remote operations (push/pull/fetch) yet — see Phase 3. Unlike the old browser build,
  this isn't blocked on a server backend; libgit2 has native transport support, so this is
  reachable directly once scheduled.
- Single active repository session at a time in the UI (multi-repo backend support — one
  worker thread per repo — already exists in `state::AppState`, but the sidebar repo switcher
  is the only multi-repo UI built so far).

### Phase 2: Enhanced Features (implemented)
- [x] Branch management (create/delete/rename/switch) — native `git2` support
- [x] Stash (push/apply/pop/drop/list) — native `git2` support
- [x] Merge with conflict resolution — native `git2` merge + conflict index reader
- [x] Interactive rebase — pick/reword/edit/squash/fixup/drop, driven by Browsitory's own code
      on top of `Repository::rebase()`'s mechanical pick-only primitive (see CLAUDE.md's git2
      gotchas for why squash/fixup/drop aren't natively supported despite the type looking like
      they should be) — still a capability gain over the old isomorphic-git-based rebase, which
      was hand-rolled and limited to pick/drop only
- [x] Blame viewer — native `Repository::blame_file()` (no hand-rolled line-attribution
      needed, unlike the old codebase)
- [x] Multi-branch commit graph view — hand-rolled lane/column layout (no off-the-shelf Rust
      equivalent of the old dagre-based layout), painted directly with `egui::Painter`

### Phase 3: Remote Operations (implemented)
- [x] Push/pull/fetch with progress reporting
- [x] Credential handling (SSH agent, platform credential managers) via `git2` callbacks
- [x] Multi-remote support (add/remove/rename/set-url, remote-CRUD UI panel)
- [x] Tag push

This replaces the old roadmap's Phase 3 "server backend" — that phase existed only because
isomorphic-git in a browser can't reach a remote directly. A native app with libgit2 doesn't
have that constraint, so there's no backend to build first.

### Phase 4: Advanced Features (not started)
- [ ] Worktrees
- [ ] Submodules
- [ ] Reflog viewer
- [ ] PR integration (GitHub/GitLab)

---

## Key Decisions & Rationale

### Why Rust + egui, not a Tauri/web-frontend hybrid?
A pure-Rust immediate-mode GUI (`egui`) avoids bundling a webview runtime and keeps the whole
stack in one language, which matches the goal of a fast, keyboard-driven tool in the Sublime
Merge spirit — Sublime Merge itself uses a custom-drawn UI, not native OS widgets or an
embedded browser.

### Why git2 (libgit2 bindings) over a pure-Rust git implementation?
`git2` is mature and complete — native blame, native interactive rebase, native merge conflict
handling, native stash/cherry-pick, native remote transports. The pure-Rust alternative
(gitoxide/gix) would have been a closer license fit, but its write-side operations (merge,
rebase) were judged less mature; `git2` was chosen as the pragmatic choice, with the libgit2
license deviation explicitly documented rather than silently accepted (see
[LICENSE_COMPLIANCE.md](LICENSE_COMPLIANCE.md)).

### Why no embedded database for caching?
`git2::Revwalk` is already a lazy iterator; paginating it directly (`skip`/`limit`) is enough
to keep commit history loading cheap without a separate cache layer to keep in sync with the
real `.git` directory.

---

## Next Steps

1. Run `scripts/setup-dev.sh` on a fresh machine
2. `cargo build --workspace && cargo run -p app`
3. Read [DEVELOPMENT.md](DEVELOPMENT.md) for conventions
4. Pick a Phase 4 feature (worktrees, submodules, reflog, PR integration) from
   [FEATURES.md](FEATURES.md) and follow the `git-core` module pattern described in
   [ARCHITECTURE.md](ARCHITECTURE.md)

---

**Status**: 🟡 Phase 1, Phase 2, and Phase 3 implemented, Phase 4 not started.
**License**: MIT (with one documented libgit2 linking exception — see
[LICENSE_COMPLIANCE.md](LICENSE_COMPLIANCE.md)).
