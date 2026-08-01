# Browsitory - Architecture & Tech Stack

## Overview

Browsitory is a native, cross-platform desktop Git client written in Rust. There is no
browser, no server, and no optional backend — a single native binary talks directly to git
repositories on the local filesystem via libgit2. This is a deliberate simplification versus
the project's original design (a browser PWA with an optional server-mode backend, described
in earlier revisions of this document — see git history before branch `feat/rust_from_scratch`
if you need that context): a native app doesn't need a File System Access API workaround, a
service worker, IndexedDB caching, or a REST API layer between the UI and git.

## Cargo workspace

Three crates:

```
browsitory/
├── Cargo.toml            # workspace manifest
├── rust-toolchain.toml   # pinned stable channel
└── crates/
    ├── git-core/         # git2-based service layer
    ├── config/           # repo registry + preferences (TOML)
    └── app/              # egui/eframe desktop UI
```

### `git-core`

Wraps `git2` (libgit2 bindings) as plain functions taking `&git2::Repository` (or a path, for
`open`) explicitly, mirroring the dependency-injection shape the original JS codebase used for
its `git.ts` service layer. This keeps the crate testable headlessly against real temp-directory
repositories (`git2::Repository::init` + `tempfile`), with no GUI and no mocking of `git2`
itself.

Modules implemented so far: `repo` (open/discover), `status`, `log` (paginated `Revwalk`),
`diff` (line-level diff + `similar`-based word-level highlighting), `stage` (stage/unstage),
`commit`. Planned for Phase 2: `branch`, `stash`, `merge`, `rebase`, `blame`, `graph` — all of
which have **native** libgit2 support (unlike the old isomorphic-git codebase, which had to
hand-roll blame and interactive rebase, with real bugs along the way).

### `config`

Repo registry (paths of previously-opened repositories) and user preferences, persisted as one
TOML file via `serde`/`toml`, in the OS-appropriate config directory (resolved by the
`directories` crate). Replaces the old IndexedDB/localStorage-based persistence — a native app
just writes a real file to a real path.

### `app`

The `egui`/`eframe` desktop UI — the only crate depending on `eframe`, `egui`, and `rfd` (native
folder-picker dialog). Structure:

- `worker.rs` — spawns one dedicated OS thread per open repository. `git2::Repository` is not
  `Send`, so each worker thread opens and owns its own `Repository` handle for its entire
  lifetime; only plain `Command`/`Event` enum values cross the thread boundary via
  `std::sync::mpsc` channels.
- `state.rs` — `AppState` (list of open `RepoSession`s + which is active) and `RepoSession`
  (per-repo view-model: cached status/log/diff, commit message buffer, sends `Command`s,
  applies incoming `Event`s to its own fields).
- `ui/` — one module per panel (`staging_panel`, `history_view`, `diff_view`; `branch_panel`,
  `stash_panel`, `conflict_view`, `graph_view` planned for Phase 2), each a plain function
  taking `&mut egui::Ui` and the relevant state.
- `main.rs` — the `eframe::App` impl and top-level panel layout.
- `theme.rs` — applies the user's theme preference to `egui::Visuals`.

## Threading model

Git operations via `git2` are blocking, and egui is immediate-mode (redraws every frame based
on current state) — so git work must happen off the UI thread. Each `RepoSession::open` spawns
a worker thread that loops on an `mpsc::Receiver<Command>`, executes the corresponding
`git-core` call, and sends an `Event` back. The worker calls `egui::Context::request_repaint()`
after each command (a cloned `Context` is `Send + Sync`) so the UI updates promptly instead of
waiting for the next input-driven repaint; the UI thread drains events non-blocking
(`try_recv`) once per frame in `RepoSession::poll_events`. One worker thread per open repository
also means switching between repos in a multi-repo session never contends on a shared
`Repository` handle.

## Data persistence

- **Repo registry & preferences**: a single TOML file (`config` crate), in the OS config
  directory.
- **Commit history**: no cache layer — `git2::Revwalk` is already a lazy iterator, so
  `git-core::commit_log`'s `skip`/`limit` pagination stays cheap without needing to
  materialize or cache the whole history, even on large repos.
- **Everything else** (working tree, index, objects, refs): the real `.git` directory on disk,
  read directly via libgit2 — there is no separate "offline cache" to keep in sync, unlike the
  old browser build's IndexedDB commit/diff cache.

## Remote operations (opportunity, not yet built)

The old architecture punted push/pull/fetch to an unbuilt "server backend" phase, because
isomorphic-git running in a browser has no direct way to talk to a remote (it would need a
CORS-enabled proxy). `git2`/libgit2 has native transport support (HTTPS via vendored OpenSSL,
SSH via vendored libssh2 — see `crates/git-core/Cargo.toml`'s `git2` features), so remote
operations are directly reachable from a native app without any backend. This is why "remote
operations" moved from Phase 3's old "server backend" framing to a plain, self-contained
Phase 3 item in the current roadmap — see `docs/PROJECT_SETUP.md`.

## Performance considerations

1. **Lazy log loading**: `Revwalk`-based pagination (`skip`/`limit`), not a full-history load.
2. **Off-thread git work**: every `git2` call runs on a repo's dedicated worker thread, never
   blocking the UI thread's frame loop.
3. **One thread per open repo**: avoids lock contention between repos in a multi-repo session,
   and makes "quick switch between repos" (a `FEATURES.md` requirement) essentially free.

## Security considerations

1. **Git credentials**: when remote operations (Phase 3) are implemented, use platform
   credential managers / SSH agent forwarding via `git2`'s credential callbacks — never store
   plaintext passwords.
2. **Input validation**: pathspecs passed to `git2` are user-supplied file paths from within an
   already-opened repository's working tree; no arbitrary shell invocation is used anywhere in
   `git-core` (all operations go through libgit2's C API via `git2`, not a spawned `git` CLI
   process), which rules out a whole class of shell-injection concerns.
