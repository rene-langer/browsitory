# Architecture

## Crate/package layout

```
browsitory/
├── crates/
│   ├── git-core/    # git2-based service layer, UI-agnostic, unit-tested headlessly
│   ├── config/      # repo registry + preferences: recent-repos list, backed by TOML
│   └── tauri-app/    # Tauri commands + per-repo worker threads
└── frontend/          # React + TypeScript + Vite, the only crate/package that talks to a UI toolkit
```

## Why Tauri + a web frontend, not egui again

A prior pass (see git history on this branch before 2026-08-11) used `egui` for a
single-language, no-webview native UI. It worked, but `egui`'s immediate-mode canvas can't be
embedded in a VSCode webview later — a stated product requirement ("frontend shall be either
the standalone app or a vscode integration"). Tauri packages a plain web frontend as a
standalone app today, and that same frontend becomes the VSCode webview later by adding one
more `RepoClient` implementation — no UI rewrite.

## Why git2 (libgit2 bindings), not gitoxide

`git2` is mature and complete: native blame, native interactive rebase primitives, native
merge conflict handling, native stash/cherry-pick, native remote transports. The pure-Rust
alternative (`gitoxide`/`gix`) is a closer license fit (no GPL exception needed at all), but
its write-side operations (merge, rebase) are less mature as of this writing. `git2` is the
pragmatic choice; the libgit2 license deviation is documented, not silently accepted — see
`CLAUDE.md`'s License policy section.

## The `RepoClient` IPC boundary

`frontend/src/ipc/RepoClient.ts` defines the interface every UI component depends on. It grows
with each feature phase (11 methods as of Phase 1: repo picking/opening, status, log, diffs,
stage/unstage, commit) — see that file directly for the current shape rather than a copy here,
which would just go stale again next phase.

`frontend/src/ipc/tauriRepoClient.ts` implements it over `@tauri-apps/api`'s `invoke()`. This
is the *only* file allowed to import `@tauri-apps/api` — every other frontend file receives a
`RepoClient` as a prop/context value, so it can't accidentally couple to Tauri. A future VSCode
extension implements the same interface over `postMessage` in a sibling file
(`vscodeRepoClient.ts`); no component changes.

The rule is enforced mechanically, not just by review: `frontend/eslint.config.js` has a
`no-restricted-imports` override banning `@tauri-apps/*` imports from
`frontend/src/components/**` and `frontend/src/state/**`, so a violation fails `pnpm lint`
(and CI).

## Threading model

`git2::Repository` **is** `Send` (libgit2 handles can be moved between threads, and `git2`
carries an `unsafe impl Send` to say so) but it is **not** `Sync`: a `&Repository` must never
be used from two threads at once. So the handle can be *given* to exactly one thread, but not
*shared*. That rules out the obvious Tauri shape — managed state requires `Send + Sync`, so a
bare `Repository` can't be `State` at all, and `State<Mutex<Repository>>` (which does satisfy
`Sync`) would funnel every concurrent command invocation through a single lock held for the
duration of blocking git work.

The `!Sync` constraint is what message-passing answers. Each opened repository gets one
dedicated OS thread (`crates/tauri-app/src/worker.rs`'s `Worker::spawn`) that opens its own
`Repository` handle and owns it exclusively for the thread's lifetime — the handle is moved in
once and never shared by reference. Tauri commands (`crates/tauri-app/src/commands.rs`) send a
`Command` enum value over a `std::sync::mpsc` channel to that thread and receive the result
over a per-call reply channel; only owned, `Send` command/reply values cross the boundary.
Commands clone the channel `Sender` out of the state mutex and drop the guard before blocking
on a reply, so one slow repository operation can't serialize unrelated commands. One worker
thread per open repo also means multiple repos never contend on a shared handle.

**Every Tauri command that blocks must be an `async fn`.** A plain `#[tauri::command] fn` is
dispatched by `tauri-macros`' `ExecutionContext::Blocking` path, which runs the function body
*inline on the main/UI thread*; an `async fn` command takes the `ExecutionContext::Async` path
(`InvokeResolver::respond_async_serialized` → `async_runtime::spawn`) and runs off the main
thread. So anything that parks the calling thread — `reply_rx.recv()` on a worker round-trip,
`tauri_plugin_dialog`'s `blocking_pick_folder()` — freezes the whole window if the command is
sync. In `blocking_pick_folder()`'s case it doesn't merely freeze, it *deadlocks*: the dialog
is posted back to the main thread via `run_on_main_thread` and then waited on, so when the main
thread is the caller the dialog can never be dispatched and the app hangs forever. Every
command in `commands.rs` that touches the worker or the dialog plugin is therefore an
`async fn`. Note that an `async fn` command taking a borrowed argument (e.g.
`state: State<'_, AppState>`) must return a `Result`, which is why they all do.

## Error handling

`git-core` functions return typed errors (`thiserror` enums per module: `RepoError`,
`StatusError`, ...). `Worker`/Tauri commands map these to `Result<T, String>` crossing the IPC
boundary (Tauri serializes `Err` as a rejected JS promise). `RepoClient` methods return
`Promise<T>` that reject with that message — no error is swallowed at the boundary.

## Testing strategy

- `git-core`/`config`: `cargo test`, real temp-dir repos/files, no mocks.
- `tauri-app`: inline unit tests for logic that isn't thin delegation (see `worker.rs`'s tests,
  which spawn a real worker thread against a real temp-dir repo). Pass-through Tauri commands
  don't get separate tests, except for the DTO wire format: `commands.rs` has a test pinning
  the `StatusKind` strings it serializes to the `StatusKind` union in
  `frontend/src/ipc/RepoClient.ts`, a contract no other test covers.
- `frontend`: Vitest + Testing Library, mocking `RepoClient` (a real interface seam).
- E2E (added from Phase 1 onward, not in Phase 0): `tauri-driver` + WebdriverIO (`e2e/`) driving
  the built `tauri-app` binary as a black box — `cargo build --workspace --features
  tauri-app/custom-protocol` (the `custom-protocol` feature makes the binary load its embedded
  `frontend/dist` instead of the Vite dev server; plain `cargo build` stays in dev-server mode
  regardless of debug/release, so it can't be driven standalone). One flow per major feature
  area, added where a flow spans backend+frontend in a way unit tests can't catch; `App.tsx`
  reads a `VITE_E2E_REPO_PATH` build-time env var to auto-open a fixture repo, since
  `RepoPicker`'s native folder dialog can't be driven through WebDriver. (Not Playwright:
  Playwright drives browser engines it manages itself and can't attach to a native
  Tauri/webkit2gtk window; `tauri-driver` is Tauri's own WebDriver bridge, the
  actually-supported E2E path.) Both `e2e/package.json` and `frontend/package.json` pin
  `packageManager: "pnpm@9.15.9"` — a CI-vs-local pnpm major-version mismatch broke `e2e/`'s
  frozen-lockfile install during Phase 1; a contributor running a different pnpm major without
  Corepack honoring this pin can hit the same failure.

## Roadmap

- **Phase 0**: workspace scaffold, `git-core::repo`/`status`, Tauri shell + minimal status view
  proving the IPC boundary.
- **Phase 1** (this pass, complete): full repo view — `git-core::log`/`diff`/`stage`/`commit`;
  `config` turned into a real recent-repos registry; `RepoPicker`/`HistoryList`/`DiffPane`/
  `CommitBox` frontend, replacing `StatusView`; first GUI E2E layer (`e2e/`, `tauri-driver` +
  WebdriverIO) plus a CI job for it. See `CLAUDE.md`'s "Project status" for the short version.
- **Phase 2**: branch management, stash, merge with conflict resolution, interactive rebase,
  blame viewer, multi-branch commit graph.
- **Phase 3**: push/pull/fetch with progress, multi-remote, tag push, credential handling.
- **Phase 4**: worktrees, submodules, reflog viewer, PR integration.
