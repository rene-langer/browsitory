# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build --workspace              # build all crates
cargo run -p app                     # launch the desktop app
cargo test --workspace                # run all tests (git-core + config + app)
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all                      # format
cargo fmt --all -- --check           # format check (used in CI)
```

Run a single test file or test: `cargo test -p git-core --test log` or
`cargo test -p git-core -- reports_untracked_file_as_unstaged_new`.

First-time setup on a fresh machine: `scripts/setup-dev.sh` installs the Rust toolchain (via
rustup) and the system packages `eframe`/`winit` and `git2`'s vendored libgit2 build need
(cmake, a C toolchain, `libxkbcommon`/`libwayland`/`libx11` dev headers on Linux).

CI (`.github/workflows/ci.yml`) runs, per OS in a Linux/macOS/Windows matrix, in order:
`cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `cargo build --release`.
All must pass on all three platforms.

## Project status

This is a from-scratch Rust rewrite (branch `feat/rust_from_scratch`) of what was previously a
browser-only PWA (React + TypeScript + isomorphic-git). The old JS/TS codebase was deleted
outright rather than ported incrementally — see git history before this branch if you need to
recover something from it.

Phase 1 is implemented: open a local repository, view status, commit history, and file diffs,
stage/unstage, and commit. Phase 2 (this pass) is also implemented: branch management, stash,
merge with conflict resolution, interactive rebase (pick/reword/edit/squash/fixup/drop), a blame
viewer, and a multi-branch commit graph view. Remote operations (push/pull/fetch) are **not**
implemented yet — see Phase 3 in `docs/PROJECT_SETUP.md` for the full phase roadmap and
`docs/ARCHITECTURE.md` for the tech-stack rationale.

## Architecture

Three-crate Cargo workspace:

- **`crates/git-core`** — all git operations, wrapping `git2` (libgit2 bindings). UI-agnostic
  and unit-tested headlessly.
- **`crates/config`** — repo registry (list of opened repo paths) + user preferences,
  persisted as a single TOML file via `serde`/`toml` in the OS config directory (resolved by
  the `directories` crate). Replaces the old browser build's IndexedDB-backed registry — a
  native app just writes a real file.
- **`crates/app`** — the `egui`/`eframe` desktop UI. The only crate that depends on
  `eframe`/`egui`/`rfd`.

This is a **native desktop app**, not a browser PWA: no File System Access API, no service
worker, no IndexedDB, no permission-revocation dance. Repository access is direct filesystem
access via `git2`, working from any OS.

### git-core is dependency-injected on purpose

Every function in `git-core` takes a `&git2::Repository` (or a path, for `open`) as an
explicit argument rather than reading a module singleton — same rationale as the old `git.ts`:
it keeps the crate unit-testable with real temp-directory repos
(`tempfile::TempDir` + `git2::Repository::init`, see `crates/git-core/tests/common/mod.rs`),
exercising the *exact same* code paths the UI uses. When adding a new git operation, keep this
shape: a plain function in its own module (`status.rs`, `log.rs`, `diff.rs`, ...), re-exported
from `lib.rs`, tested against a real repo in `tests/`, not a mocked `git2::Repository`.

### git2 API gotchas already hit once

- Several `git2` accessors that look infallible aren't: `Commit::summary()` and
  `Signature::name()`/`email()` return `Result<Option<&str>, Error>` /
  `Result<&str, Error>`, not bare `Option`/`&str` — chain `.ok().flatten().unwrap_or_default()`
  or `.unwrap_or_default()` accordingly (see `log.rs`). `StatusEntry::path()` returns
  `Result<&str, Error>`, not `Option<&str>` — use `let Ok(path) = entry.path() else { continue };`
  in a status loop, not `let Some(...)`.
- `Revwalk::set_sorting` with plain `Sort::TIME` is not enough to guarantee parents sort after
  children — commits made in the same second (as fast test setups do, and as real rebases/
  imports can) can come out in the wrong order. Use `Sort::TOPOLOGICAL | Sort::TIME` (matches
  `git log`'s own default ordering) — see `log.rs`.
- `Reference::shorthand()` (and thus `Branch::get().shorthand()`/`repo.head()?.shorthand()`)
  is another one of these: `Result<&str, Error>`, not `Option<&str>` — it only errs when the
  name isn't valid UTF-8. Handle it the same way as the other gotchas above (`.ok()` to fold
  into an `Option`, or an explicit `let Ok(name) = ... else { continue }` in a loop) — see
  `branch.rs`.

### git2 vs. isomorphic-git: capability gain, not just a port

libgit2 has **native** support for several things the old isomorphic-git codebase had to
hand-roll (and where the hand-rolling had real bugs — see the old CLAUDE.md via git history if
curious): blame (`Repository::blame_file`), native merge conflict indices
(`Index::conflicts()`), and native stash/cherry-pick. Don't re-introduce hand-rolled versions of
these; use the native `git2` API.

`Repository::rebase()` is a partial exception, worth knowing before touching `rebase.rs`:
libgit2's own rebase driver (traced into `libgit2-sys`'s vendored C source) only ever generates
`Pick` operations — the `git_rebase_operation_t` enum has `Reword`/`Edit`/`Squash`/`Fixup`/`Exec`
variants for API completeness, but nothing in git2 produces them, and there's no way to hand it a
custom todo list. `Rebase::next()` always means "mechanically cherry-pick the next commit
onto whatever `commit()` last produced" — reword/edit/squash/fixup/drop are all driven by
Browsitory's own code on top of that one primitive (see `rebase.rs`'s module doc comment for the
per-action mechanics, including the squash/fixup commit-and-replace technique and why
`Repository::reset` can't be used mid-rebase — it unconditionally calls libgit2's
`git_repository_state_cleanup()`, silently wiping in-progress rebase state). Still a genuine
capability gain over the old isomorphic-git-based rebase (which was limited to pick/drop only,
with no reword/edit/squash/fixup at all) — just not a case of "git2 does it for you."

### Threading model

`git2::Repository` is not `Send`. Each open repository (`state::RepoSession`) spawns one
dedicated worker thread (`worker::spawn` in `crates/app/src/worker.rs`) that opens its own
`Repository` handle and owns it exclusively for the thread's lifetime — the handle itself never
crosses a thread boundary, only plain owned `Command`/`Event` enum values do, over
`std::sync::mpsc` channels. The UI thread (`eframe::App::ui`) drains the event channel
non-blocking (`try_recv`) each frame; the worker calls `egui::Context::request_repaint()`
(a cloned `Context` is `Send + Sync`) after finishing a command so the UI updates promptly
instead of waiting for the next input-driven repaint. One worker thread per open repo also
means switching between repos never contends on a shared handle — this is what will make
multi-repo "quick switch" (a `FEATURES.md` requirement) essentially free later.

When adding a new UI-triggered git operation: add a `Command`/`Event` variant pair in
`worker.rs`, handle it in `worker::handle`, and add the corresponding state mutation in
`RepoSession::poll_events` (`crates/app/src/state.rs`) — don't call `git-core` functions
directly from UI code in `crates/app/src/ui/`.

### egui/eframe version-specific API notes (0.35)

This version's `App` trait's required method is `fn ui(&mut self, ui: &mut egui::Ui, frame:
&mut eframe::Frame)`, not the older `update(&mut self, ctx: &egui::Context, frame: ...)`.
`SidePanel`/`TopBottomPanel` from older egui versions don't exist in 0.35 — they were unified
into a single `egui::Panel` type with `Panel::left/right/top/bottom(id)` constructors, and
every panel's `.show()` (including `CentralPanel`) now takes `&mut Ui`, not `&Context`. Panels
are still shown by nesting `.show(ui, |ui| ...)` calls against the same outer `ui` in sequence
(side/top/bottom panels first, `CentralPanel` last) — see `crates/app/src/main.rs`. If you're
looking at online egui examples that use `ctx.set_visuals`/`SidePanel::show(ctx, ...)`, they're
likely for an older version; check `crates/app/Cargo.toml`'s pinned egui/eframe version and the
vendored source under `~/.cargo/registry/src/.../egui-<version>` before trusting example code.

## License policy

Permissive dependencies only (MIT, Apache-2.0, ISC, BSD, MIT-0) with **one explicit, deliberate
exception**: `git2` links against libgit2 (via vendored build), which is
GPL-2.0-with-linking-exception — not MIT, but the linking exception explicitly permits linking
from differently-licensed code, so this was a conscious choice (see `docs/LICENSE_COMPLIANCE.md`
for the full rationale) rather than an oversight. Verify new dependencies with
`cargo info <crate>` and record them in `docs/LICENSE_COMPLIANCE.md`.

## Testing conventions

- `git-core` tests live in `crates/git-core/tests/*.rs` (integration tests, one file per
  module) plus a shared `tests/common/mod.rs` helper (`init_repo()` inits a temp-dir repo with
  a test identity configured; `write_file`/`remove_file` mutate its working tree). They use
  real repos via `git2::Repository::init`, never a mocked `Repository` — direct continuation
  of the old `git.test.ts` philosophy.
- `config` tests are unit tests inside `store.rs` (`#[cfg(test)] mod tests`), also against a
  real temp-dir TOML file, not a mocked filesystem.
- `app` has no tests yet; when adding some, prefer testing `state.rs`'s event-handling logic
  (`RepoSession::poll_events`, `AppState::open_repo`) directly as plain Rust, rather than trying
  to drive `eframe`'s windowing/GPU context in a test.
