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

```bash
# E2E (tauri-driver + WebdriverIO), from the repo root:
cargo install tauri-driver --locked               # once, if not already installed
cd frontend && VITE_E2E_REPO_PATH=/tmp/browsitory-e2e-repo pnpm build && cd ..
cargo build --workspace --features tauri-app/custom-protocol
cd e2e
pnpm install
pnpm test                                          # spawns/reaps tauri-driver itself; needs a display (xvfb-run on headless CI)
```

## Project status

Second from-scratch rewrite (branch `feat/rust_from_scratch`). See
`docs/superpowers/specs/2026-08-11-browsitory-architecture-design.md` for the full rationale —
in short: a prior native Rust+egui pass (also on this branch, see git history) worked but
produced a UI that can't be reused as a VSCode webview, which is a stated future requirement.
This pass keeps the Rust git layer but replaces egui with Tauri + a React/TypeScript frontend
behind a `RepoClient` IPC interface, so a VSCode extension can implement the same interface
later without touching UI code.

Phase 0 was setup only: workspace scaffold, CI, `git-core::repo`/`status` with tests, and a
Tauri shell proving the IPC boundary end-to-end with a minimal status view.

Phase 1 (this pass) is complete: full repo view. Added `git-core::log` (commit history),
`git-core::diff` (line-level diffs for both the working tree and a given commit, plus a
`commit_files` helper), `git-core::stage` (whole-file stage/unstage), and `git-core::commit`
(commit creation) to the git layer; turned `crates/config` from a stub into a real recent-repos
registry backed by TOML; added 9 Tauri commands and a `tauri-plugin-dialog`-backed folder
picker; and built the unified frontend layout (`RepoPicker`, `HistoryList`, `DiffPane`,
`CommitBox`, composed in `App.tsx`) with basic keyboard navigation, retiring the old
`StatusView`. Also added Browsitory's first GUI E2E layer (`e2e/`, see "Testing conventions"
below) and a CI job for it. Phase 2 (branch management, stash, merge, rebase, blame, multi-branch
graph) is next and not started — see `docs/ARCHITECTURE.md`'s Roadmap.

## Architecture

See `docs/ARCHITECTURE.md` for the full crate/package layout, the `RepoClient` IPC boundary,
and the threading model. Summary: `crates/git-core` (git2, UI-agnostic, DI'd per function,
tested against real temp-dir repos) + `crates/config` (TOML-backed recent-repos registry) +
`crates/tauri-app` (Tauri commands, one worker thread per open repo) + `frontend/` (React/TS,
talks to the backend only through `frontend/src/ipc/RepoClient.ts`).

Building `tauri-app` standalone (no dev server) requires the `custom-protocol` Cargo feature —
see the "Commands" section's E2E block and `crates/tauri-app/Cargo.toml`'s comment on it. Plain
`cargo build --workspace` always leaves the binary looking for the Vite dev server, regardless
of debug/release.

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
- `e2e/` holds `tauri-driver` + WebdriverIO specs (`e2e/specs/*.spec.ts`) that drive the real
  built `tauri-app` binary as a black box, one flow per major feature area (currently: open
  repo → stage a file → commit → see it in history). Run separately from `cargo test`/`pnpm
  test` — it needs a debug build with the `custom-protocol` feature and a frontend build with
  `VITE_E2E_REPO_PATH` baked in first; see the "Commands" section above for the exact sequence
  (mirrors `.github/workflows/ci.yml`'s `e2e` job, the source of truth if this drifts).

## Task workflow

This repo uses the `superpowers` plugin's `test-driven-development`, `writing-plans`,
`subagent-driven-development`, and `executing-plans` skills for all implementation work, plus
the project-local `.claude/skills/browsitory-conventions` skill for the Browsitory-specific
conventions those global skills don't cover (real temp-dir repos in tests, the `RepoClient`
transport-isolation rule, task-file naming). That skill is a pointer into this file and
`docs/ARCHITECTURE.md`, which stay authoritative if the two ever disagree. New implementation
tasks (Phase 1 onward) follow `docs/TASK_TEMPLATE.md`.
