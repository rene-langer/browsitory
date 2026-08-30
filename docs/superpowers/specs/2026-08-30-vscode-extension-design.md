# Browsitory as a VSCode extension — design

Status: approved, not yet planned/implemented.

## Purpose

Ship Browsitory as a VSCode extension (desktop only — vscode.dev/Codespaces web out of
scope) without abandoning the Rust `git-core` layer or the React frontend. This is the
"future requirement" `docs/ARCHITECTURE.md` already names as the reason the app is
Tauri + `RepoClient` instead of egui: a second `RepoClient` implementation should be
enough, with no UI rewrite.

## Constraints / decisions

- **Desktop VSCode only.** The extension host is a Node.js process on the user's
  machine, so it can spawn native processes and reach the OS keychain/filesystem like
  the desktop app does. No pure-JS/WASM git reimplementation, no vscode.dev support.
- **Sidecar process, not a native Node addon.** The Rust side ships as a standalone
  binary the extension host spawns and talks to over stdio, not a compiled `.node`
  addon loaded in-process. Avoids coupling to Node's ABI/N-API version per
  platform/arch, and a sidecar crash can't take the extension host down.
- **Full RepoClient parity is the target**, not an MVP subset — the ~85-method surface
  (status/log/diff/stage/commit, branches, worktrees, submodules, reflog,
  remotes/push/pull/credentials, stash, blame, merge, rebase, PR integration) ships in
  full, because the sidecar is mostly plumbing over already-tested `git-core`/`Worker`
  logic, not new feature work.
- **Full custom webview**, not VSCode's native Source Control (SCM) sidebar. Reuses the
  existing React frontend's Sublime-Merge-style commit/history/diff/staging UI
  unmodified, rather than building and maintaining a second, thinner UI against
  VSCode's SCM API.
- **Config, recent repos, and saved remote credentials are shared** between the
  desktop app and the extension — both read/write the same TOML config file and the
  same OS keychain entries (via the existing `config` and `keyring`-backed credential
  store), since both are desktop-local. No separate config-dir subpath or keyring
  service name for the extension variant.

## Architecture

```
extension host (Node) ──spawns──> vscode-sidecar (Rust binary)
        │  postMessage                    │ line-delimited JSON-RPC 2.0 over stdio
        ▼                                  ▼
   webview (React, same frontend/dist)   Worker threads (git2), one per open repo
```

`extension/` (new, TypeScript, sibling to `frontend/`) is the VSCode extension host. It
hosts the webview (the existing `frontend/dist` build, unmodified) and owns the sidecar
process's lifecycle: one sidecar process per VSCode window, spawned on first
repo-related activation, reusing the existing one-worker-thread-per-open-repo model
inside it. Architecture inside the sidecar is unchanged from the desktop app — only the
transport in front of it is new.

`frontend/src/ipc/vscodeRepoClient.ts` (new, sibling to `tauriRepoClient.ts`)
implements `RepoClient` by `postMessage`-ing to the extension host, which forwards the
call as a JSON-RPC request to the sidecar over stdio and relays the response back to
the webview via `postMessage`. `subscribeTransferProgress` maps to JSON-RPC
notifications (server-initiated, no request id) pushed the same way `TransferProgress`
events already flow through the Tauri event system today. No component in
`frontend/src/components`/`frontend/src/state` changes — same rule the existing
`no-restricted-imports` ESLint override enforces for `tauriRepoClient.ts`, extended to
also ban `vscode`-API imports outside `vscodeRepoClient.ts` and `extension/`.

### Sharing the dispatch logic (`repo-service`)

Today `crates/tauri-app/src/worker.rs` (`Worker::spawn`, the per-repo owning thread) and
`crates/tauri-app/src/commands.rs` (the `Command` enum and Tauri command functions) mix
transport-agnostic dispatch logic with Tauri-specific glue. To avoid re-authoring
~85 methods' dispatch twice, extract the transport-agnostic parts into a new crate,
`crates/repo-service`:

- The `Command` enum (one variant per `RepoClient` method) and its `git-core` dispatch.
- `Worker::spawn` and the per-repo thread + `mpsc` reply-channel machinery.
- The DTO wire-format test that today pins `commands.rs`'s `StatusKind` strings against
  `frontend/src/ipc/RepoClient.ts`'s `StatusKind` union — moves down into
  `repo-service` so every transport adapter inherits the same contract check.

`tauri-app`'s `commands.rs` and the new sidecar's JSON-RPC handler both become thin
adapters: map their transport's request shape to a `Command`, send it to `repo-service`,
serialize the reply. This is a refactor of existing tested code, not new logic — the
desktop app must keep passing its existing test suite unchanged after the extraction.

### VSCode-native integrations (extension-side, no sidecar involvement)

Tauri-plugin-backed `RepoClient` methods get replaced by native VSCode APIs, called
directly from `extension/`, never round-tripped through the sidecar:

- `pickRepoFolder` → `vscode.window.showOpenDialog`
- `openExternalUrl` → `vscode.env.openExternal`
- `getAppVersion` → reads `extension/package.json`'s version.
- `getLastSeenVersion`/`setLastSeenVersion` → `ExtensionContext.globalState`, not the
  sidecar. The rest of the Tauri auto-updater flow (`tauri-plugin-updater`) has no
  VSCode equivalent needed at all — the marketplace handles extension updates.

Everything else (credentials via `keyring`, config/recent-repos/workspaces via the TOML
file, all git operations) stays in the sidecar, reachable because both apps run
desktop-local.

## Packaging & distribution

Platform-specific `.vsix` packages (`vsce package --target <triple>`), each bundling
the prebuilt sidecar binary for that target — same pattern rust-analyzer's own
extension uses for its native binary. CI adds a build matrix producing one
`vscode-sidecar` binary per target triple, alongside the existing
`scripts/build-dist.*` desktop build jobs.

## Error handling

Unchanged in spirit from the Tauri app: `git-core` typed errors surface as `Result<T,
String>` at the `repo-service` boundary; the JSON-RPC adapter maps that to a JSON-RPC
error response, and `vscodeRepoClient.ts` rejects the `Promise<T>` with that message,
same contract `RepoClient` callers already rely on. Sidecar process crashes/exits are
new failure modes the Tauri transport doesn't have — `extension/` must detect sidecar
exit and surface a "reconnecting/failed" state rather than hanging pending requests
forever; exact UX for this is left to the implementation plan for sub-phase (c) below.

## Testing

- `repo-service` inherits `tauri-app`'s existing inline `Worker`/dispatch tests
  (real temp-dir repos, no mocks) once that logic moves there.
- The sidecar's JSON-RPC adapter gets thin pass-through tests only, same rationale
  `tauri-app/commands.rs` uses today for not testing pure delegation.
- New E2E layer under `extension/e2e/`, using `@vscode/test-electron` to drive the
  packaged extension inside a real VSCode instance — same "one flow per major feature
  area" convention as `e2e/`, starting with open repo → stage a file → commit → see it
  in history, mirroring `e2e/specs/`'s existing first flow.

## Roadmap placement

This is Phase 6, broken into sub-phases so it doesn't land as one giant plan:

- **(a)** Extract `repo-service` out of `tauri-app`. Pure refactor — the desktop app's
  existing test suite (`cargo test --workspace`, `e2e/`) must keep passing unchanged.
- **(b)** `crates/vscode-sidecar` + the JSON-RPC protocol + `vscodeRepoClient.ts`,
  proven end-to-end on a handful of methods (e.g. status/log/diff) before wiring all 85.
- **(c)** `extension/` host + webview wiring + the native VSCode integrations
  (folder picker, external URL, version tracking) + sidecar lifecycle/crash handling.
- **(d)** Packaging/CI: per-target-triple `.vsix` builds bundling the matching sidecar
  binary.
- **(e)** `extension/e2e/` harness via `@vscode/test-electron`.

Each sub-phase gets its own implementation plan via the writing-plans skill, following
this repo's existing per-phase plan convention; this document is the spec for all of
Phase 6, not just sub-phase (a).
