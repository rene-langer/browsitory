# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build --workspace                          # build all Rust crates
cargo test --workspace                            # run all Rust tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check                        # format check (CI)
cargo tauri dev                                    # run the desktop app (from crates/tauri-app)
```

```bash
cd frontend
pnpm install
pnpm dev                                           # Vite dev server (used by `cargo tauri dev`)
pnpm build
pnpm lint
pnpm test -- --run
```

Run a single Rust test: `cargo test -p git-core --test status` or
`cargo test -p git-core -- reports_an_untracked_file_as_unstaged_new`.

## Project status

Second from-scratch rewrite (branch `feat/rust_from_scratch`). See
`docs/superpowers/specs/2026-08-11-browsitory-architecture-design.md` for the full rationale —
in short: a prior native Rust+egui pass (also on this branch, see git history) worked but
produced a UI that can't be reused as a VSCode webview, which is a stated future requirement.
This pass keeps the Rust git layer but replaces egui with Tauri + a React/TypeScript frontend
behind a `RepoClient` IPC interface, so a VSCode extension can implement the same interface
later without touching UI code.

Phase 0 (this pass) is setup only: workspace scaffold, CI, `git-core::repo`/`status` with
tests, and a Tauri shell proving the IPC boundary end-to-end with a minimal status view.
Phases 1-4 (see `docs/ARCHITECTURE.md`) are not started.

## Architecture

See `docs/ARCHITECTURE.md` for the full crate/package layout, the `RepoClient` IPC boundary,
and the threading model. Summary: `crates/git-core` (git2, UI-agnostic, DI'd per function,
tested against real temp-dir repos) + `crates/config` (TOML registry/prefs, stub so far) +
`crates/tauri-app` (Tauri commands, one worker thread per open repo) + `frontend/` (React/TS,
talks to the backend only through `frontend/src/ipc/RepoClient.ts`).

### git2 API gotchas

- `StatusEntry::path()` (and `Signature::name()`/`email()`, `Reference::shorthand()`,
  `Commit::summary()`) return `Result<&str, Error>` or `Result<Option<&str>, Error>`, not a
  bare `Option`/`&str` — verified against the vendored `git2` 0.21 source. Handle with
  `let Ok(x) = ... else { continue };` in a loop, or `.ok().flatten().unwrap_or_default()`
  otherwise. See `crates/git-core/src/status.rs`.
- `StringArray::iter()` (from `Repository::remotes()`) yields `Result<Option<&str>, Error>`
  per slot — needs `.iter().flatten().flatten()`, not a single `.flatten()`, once remote
  support is added.

### Threading model

`git2::Repository` **is** `Send` but is **not** `Sync`. It can be moved into one thread and
owned there (that's why `Worker::spawn`'s `thread::spawn(move || …)` compiles), but a
`&Repository` can never be shared across threads. Tauri managed state requires `Send + Sync`,
so a `Repository` can't be `State` directly, and putting it behind `State<Mutex<Repository>>`
would serialize every command on one lock held across blocking git work. The response to
`!Sync` is therefore message-passing to a single owning thread:
`crates/tauri-app/src/worker.rs`'s `Worker::spawn` opens the repository on a dedicated thread
and owns it for that thread's lifetime; Tauri commands (`crates/tauri-app/src/commands.rs`)
send `Command`s over an `mpsc` channel and get replies over a per-call reply channel. UI code
never touches `git-core` directly — only through `RepoClient` → a Tauri command → the worker
thread.

### `RepoClient`: why it exists

`frontend/src/ipc/RepoClient.ts` is the only interface `frontend/src/components` and
`frontend/src/state` are allowed to depend on for backend calls.
`frontend/src/ipc/tauriRepoClient.ts` is the only file that imports `@tauri-apps/api`; a
`no-restricted-imports` override in `frontend/eslint.config.js` fails `pnpm lint` if any file
under `src/components/` or `src/state/` imports `@tauri-apps/*` directly. When a
VSCode extension frontend is built later, it gets a second implementation
(`frontend/src/ipc/vscodeRepoClient.ts`, over `postMessage`) behind the same interface — no
changes to any component.

## License policy

Permissive dependencies only (MIT, Apache-2.0, ISC, BSD, MIT-0) with **one explicit, deliberate
exception**: `git2` links against libgit2 (via vendored build), which is
GPL-2.0-with-linking-exception — not MIT, but the linking exception explicitly permits linking
from differently-licensed code. Verify new dependencies (`cargo info <crate>` / `npm info
<package>`) before adding them and record them in `docs/LICENSE_COMPLIANCE.md`.

## Testing conventions

- `git-core` tests live in `crates/git-core/tests/*.rs` (one file per module) plus a shared
  `tests/common/mod.rs` helper. They use real repos via `git2::Repository::init`/`TempDir`,
  never a mocked `Repository`.
- `tauri-app` tests live inline (`#[cfg(test)] mod tests`) next to the code they test (see
  `worker.rs`), also against real temp-dir repos. Thin pass-through Tauri commands
  (`commands.rs`) don't need their own tests — the `git-core`/`Worker` logic they call already
  is tested. The exception is the DTO wire format: `commands.rs`'s test module asserts the
  `StatusKind` strings it emits match the `StatusKind` union in
  `frontend/src/ipc/RepoClient.ts`, since nothing else catches that drift.
- `frontend` tests mock `RepoClient` (a real interface seam), never `@tauri-apps/api`.

## Task workflow

This repo uses the `superpowers` plugin's `test-driven-development`, `writing-plans`,
`subagent-driven-development`, and `executing-plans` skills for all implementation work, plus
the project-local `.claude/skills/browsitory-conventions` skill for the Browsitory-specific
conventions those global skills don't cover (real temp-dir repos in tests, the `RepoClient`
transport-isolation rule, task-file naming). That skill is a pointer into this file and
`docs/ARCHITECTURE.md`, which stay authoritative if the two ever disagree. New implementation
tasks (Phase 1 onward) follow `docs/TASK_TEMPLATE.md`.
