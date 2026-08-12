# Architecture

## Crate/package layout

```
browsitory/
├── crates/
│   ├── git-core/    # git2-based service layer, UI-agnostic, unit-tested headlessly
│   ├── config/      # repo registry + preferences (TOML), stub until Phase 1
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

`frontend/src/ipc/RepoClient.ts` defines the interface every UI component depends on:

```ts
export interface RepoClient {
  openRepo(path: string): Promise<void>;
  getStatus(): Promise<StatusEntry[]>;
  // grows with each feature phase
}
```

`frontend/src/ipc/tauriRepoClient.ts` implements it over `@tauri-apps/api`'s `invoke()`. This
is the *only* file allowed to import `@tauri-apps/api` — every other frontend file receives a
`RepoClient` as a prop/context value, so it can't accidentally couple to Tauri. A future VSCode
extension implements the same interface over `postMessage` in a sibling file
(`vscodeRepoClient.ts`); no component changes.

## Threading model

`git2::Repository` is not `Send`, so it can't be shared across threads or stored in Tauri's
async command handlers directly. Each opened repository gets one dedicated OS thread
(`crates/tauri-app/src/worker.rs`'s `Worker::spawn`) that opens its own `Repository` handle and
owns it exclusively. Tauri commands (`crates/tauri-app/src/commands.rs`) send a `Command` enum
value over a `std::sync::mpsc` channel to that thread and receive the result over a per-call
reply channel — the `Repository` handle itself never crosses a thread boundary, only owned,
`Send` command/reply values do. One worker thread per open repo also means multiple repos never
contend on a shared handle.

## Error handling

`git-core` functions return typed errors (`thiserror` enums per module: `RepoError`,
`StatusError`, ...). `Worker`/Tauri commands map these to `Result<T, String>` crossing the IPC
boundary (Tauri serializes `Err` as a rejected JS promise). `RepoClient` methods return
`Promise<T>` that reject with that message — no error is swallowed at the boundary.

## Testing strategy

- `git-core`/`config`: `cargo test`, real temp-dir repos/files, no mocks.
- `tauri-app`: inline unit tests for logic that isn't thin delegation (see `worker.rs`'s tests,
  which spawn a real worker thread against a real temp-dir repo). Pass-through Tauri commands
  don't get separate tests.
- `frontend`: Vitest + Testing Library, mocking `RepoClient` (a real interface seam).
- E2E (added from Phase 1 onward, not in Phase 0): Playwright against the `cargo tauri dev`
  build, one flow per major feature area, added where a flow spans backend+frontend in a way
  unit tests can't catch.

## Roadmap

- **Phase 0** (this pass): workspace scaffold, `git-core::repo`/`status`, Tauri shell + minimal
  status view proving the IPC boundary.
- **Phase 1**: full repo view — commit history, diff viewer, stage/unstage, commit.
- **Phase 2**: branch management, stash, merge with conflict resolution, interactive rebase,
  blame viewer, multi-branch commit graph.
- **Phase 3**: push/pull/fetch with progress, multi-remote, tag push, credential handling.
- **Phase 4**: worktrees, submodules, reflog viewer, PR integration.
