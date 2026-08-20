# Multi-Repo Tabs Design

## Overview

Browsitory currently assumes exactly one open repository, enforced at both ends of the
stack: the backend (`crates/tauri-app/src/commands.rs`) holds one `Mutex<Option<Worker>>`,
and `open_repo` replaces it — opening a second repo silently drops the first. The frontend
mirrors this with one `useAppState` hook instance holding one flat state blob.

This design adds real multi-repo support: several repositories open and live at once, each
with its own background worker and its own state, switchable through a tab strip in the
header. Switching tabs is instant (no reload), and background tabs keep their state current
for in-flight operations (a push/pull/fetch started before switching away keeps running and
finishes normally).

## Goals

- Open several repositories at once; each keeps its own live worker/state in the background.
- Switch between them via a tab strip in the header, with no perceptible reload delay.
- Restore the full set of open tabs (and which one was active) on the next launch.
- Every existing single-repo feature (staging, branches, remotes, rebase, etc.) keeps working
  unchanged per tab — this is a state-scoping change, not a feature change.

## Non-goals

- Cross-repo operations (e.g. cherry-picking a commit from one open repo's tab into another).
- Any UI for relating tabs to each other (e.g. grouping, "workspace" presets). Tabs are
  independent; the only relationship between them is that they're all open in the same window.
- Changing how a single repo's commands work internally — this only threads an existing repo
  identity through calls that were implicitly single-repo before.

## Current State

- `crates/config/src/lib.rs`: a small dedicated crate persisting one thing today — a
  deduplicated, most-recent-first `recent_repos: Vec<PathBuf>` list (max 10) in
  `<os-config-dir>/config.toml`, via `list_recent_repos`/`add_recent_repo`.
- `crates/tauri-app/src/commands.rs`: `AppState { worker: Mutex<Option<Worker>> }`.
  `open_repo(path)` spawns a `Worker` and replaces whatever was in the mutex. Every other
  command (`get_status`, `stage_file`, `create_branch`, ~67 commands total) calls
  `worker_handle(&state)`, which locks the mutex and returns a handle to "the" worker,
  erroring `"no repo open"` if none is set. Transfer commands (`fetch_remote`,
  `push_current_branch`, etc.) already return a per-call `operation_id` and emit progress
  events under that id, not a fixed name — already safe for concurrent operations across
  repos with no change needed.
- `frontend/src/ipc/RepoClient.ts`: an interface of ~67 methods, none taking a repo
  identifier — every call implicitly targets "the" open repo.
- `frontend/src/ipc/tauriRepoClient.ts`: the `RepoClient` implementation, each method a thin
  `invoke("command_name", { ...args })` call.
- `frontend/src/state/useAppState.ts`: one 792-line hook, one `AppState` object
  (`repoPath`, `status`, `commits`, `branches`, ... one field per domain), called once from
  `App.tsx`.
- `frontend/src/App.tsx`: renders the repo picker when `appState.state.repoPath === null`,
  otherwise the full three-column layout (`Sidebar` | `CommitGraph` | `DiffPane`) driven by
  that single `appState`.

## Architecture

### Backend: worker registry

`AppState` changes from a single optional worker to a registry:

```rust
#[derive(Default)]
pub struct AppState {
    pub workers: Mutex<HashMap<String, Worker>>,
}
```

Keyed by the repo's canonicalized absolute path (as a `String`) — no new id scheme. A path
already uniquely identifies a repo on one machine, and it's what `config::recent_repos`
already keys on, so no separate id-allocation/mapping table is needed anywhere in the stack.

- `open_repo(path)`: canonicalizes `path`, and if that key isn't already in the map, spawns a
  `Worker` and inserts it. If it's already open, this is a no-op (idempotent) — the frontend
  uses this to mean both "open a new tab" and "focus an already-open one."
- A new `close_repo(path)` command removes the entry and drops it. `Worker` (`worker.rs`) is
  just `{ tx: Sender<Command> }` with no explicit `Drop` impl — its background thread runs
  `for command in rx { ... }`, so dropping the `Worker` (and with it, `tx`) closes the
  channel, the `for` loop ends, and the thread exits on its own. This is the same mechanism
  that already ends a `Worker`'s thread today when `open_repo` replaces the single slot; this
  design doesn't change it, only how many workers can exist at once.
- `worker_handle(&state)` becomes `worker_handle(&state, repo_path: &str)`, looking up that
  one key instead of unwrapping the single slot. Its error for a missing key changes from
  `"no repo open"` to `"repo not open: {repo_path}"`.
- Every other `#[tauri::command]` function in `commands.rs` (~67, one per `RepoClient`
  method) gains a leading `repo_path: String` parameter and passes it to `worker_handle`.
  This is a large mechanical diff but a uniform one — no command's internal logic changes.

### IPC surface

`RepoClient` (`frontend/src/ipc/RepoClient.ts`) gains a leading `repoPath: string` parameter
on every method that isn't already repo-scope-free (`pickRepoFolder`, `listRecentRepos`,
`openRepo` itself takes the path as its existing single arg already). `tauriRepoClient.ts`
passes it through as an extra `invoke` argument. This is the frontend half of the same
mechanical change — no method's call shape changes beyond the added parameter.

### Frontend: per-repo state

`useAppState(client, repoPath)` keeps its current internal shape (same `AppState` fields,
same ~35 action methods) — it's parameterized by which repo it's talking to, not restructured.

A new hook, `useOpenRepos()`, owns the tab list itself:

```ts
interface OpenRepo {
  path: string;
  displayName: string; // basename(path)
}

interface UseOpenReposResult {
  openRepos: OpenRepo[];
  activePath: string | null;
  openRepo(path: string): void;   // opens a new tab, or focuses it if already open
  closeRepo(path: string): void;
  switchTo(path: string): void;
}
```

`App.tsx` restructures around it: for each `openRepos` entry it mounts one child component
(e.g. `RepoWorkspace`) that calls `useAppState(client, repoPath)` internally and renders
today's three-column layout — all mounted simultaneously, but only the one matching
`activePath` visible (CSS, not conditional mounting), so inactive tabs' React state and
in-flight IPC calls survive a switch. The repo-picker screen (today's `repoPath === null`
branch) becomes what's shown when `openRepos.length === 0`, reached via `useOpenRepos().openRepo`
instead of `appState.openRepo` directly.

### Persistence

`crates/config/src/lib.rs`'s `ConfigFile` gains two fields:

```rust
#[derive(Debug, Default, Serialize, Deserialize)]
struct ConfigFile {
    #[serde(default)]
    recent_repos: Vec<PathBuf>,
    #[serde(default)]
    open_repos: Vec<PathBuf>,
    #[serde(default)]
    active_repo: Option<PathBuf>,
}
```

New functions `list_open_repos() -> Result<(Vec<PathBuf>, Option<PathBuf>), ConfigError>` and
`set_open_repos(paths: &[PathBuf], active: Option<&Path>) -> Result<(), ConfigError>`,
following the same `_at`-suffixed testable-seam pattern as `list_recent_repos`/`add_recent_repo`.
Two new Tauri commands wrap these. `useOpenRepos()` calls `set_open_repos` after every
open/close/switch (these are rare, user-driven actions — no debouncing needed), and calls
`list_open_repos` once on mount to restore tabs. `add_recent_repo` keeps firing on open exactly
as it does today — "recent" and "open" are separate, independently useful lists (closing a tab
should still leave it in "recent").

The existing `VITE_E2E_REPO_PATH` auto-open in `App.tsx` (used only by the E2E build) keeps
working as today, bypassing the persisted-tabs restore — E2E fixtures need a deterministic
single starting repo, not whatever tabs happen to be persisted from a prior run.

### Tab strip UI

A new `RepoTabs` component renders in `App.tsx`'s header, alongside the existing theme
toggle: one tab per `openRepos` entry (label = `displayName`, full `path` as a tooltip), a
close control per tab, and a trailing `+` that opens today's `RepoPicker` (folder dialog +
recent-repo list) in an overlay to add a tab. Picking a path already present in `openRepos`
focuses that tab (via `openRepo`'s idempotent-focus behavior) instead of opening a duplicate.

The command palette (`frontend/src/lib/commands.ts`) gains one "Switch to `<repo
displayName>`" entry per open tab, built the same way its existing "Switch to `<branch>`"
entries are.

## Edge Cases

- **Closing the active tab**: switch to the tab to its right, or to its left if it was the
  last one; closing the only remaining tab returns to the empty repo-picker screen.
- **Closing a tab with a transfer in flight**: reuse the existing `repositoryOperationDisabled`
  guard, scoped to that tab's own `appState` instead of a single app-wide flag — `closeRepo`
  is disabled (or ignored) while that repo's `transfer`/`mergeMessage`/`rebaseProgress` is
  non-null, same rule that already disables other mutating actions during those states today.
- **Mid-rebase/mid-merge tabs**: no special handling — that state lives on disk (`.git`'s
  `MERGE_HEAD`/rebase-in-progress files), so a tab can close and reopen (or restore on next
  launch) without losing it; `useAppState`'s existing status-refresh logic already detects and
  displays it on open, same as it does after an external `git rebase` today.
- **Opening a path that's already open**: `openRepo` focuses the existing tab rather than
  creating a second one for the same path (enforced by the backend's idempotent `open_repo`
  and the frontend's `openRepos` de-duplication by path).

## Testing

- **Rust**: `commands.rs`'s existing test module continues to exercise each command, now
  passing an explicit `repo_path`; a new test opens two repos into the registry and asserts
  they get independent `Worker`s (e.g. staging a file in one doesn't touch the other's status).
- **Frontend unit**: `useOpenRepos.test.ts` covers open/close/switch/restore and the
  focus-on-duplicate-open behavior in isolation (mocked `RepoClient`). `RepoTabs.test.tsx`
  covers rendering, close-button wiring, and the `+` button opening `RepoPicker`.
- **E2E**: a new `multi-repo.spec.ts` fixture sets up two separate fixture repos (not two
  branches of one repo). Covers: opening a second repo as a new tab, switching between them,
  per-tab state isolation (stage a file in tab A, assert tab B's status is unaffected),
  closing a tab, and restart-restore (relaunching the app and asserting both tabs reopen with
  the previously-active one focused).
- The `repo_path`-threading change to the ~67 existing commands is covered by the existing
  E2E suite continuing to pass unchanged — every existing spec opens exactly one repo, so
  each call site gains one argument with no behavioral difference for a single-tab session.

## Rollout

This is one implementation plan, not several sub-projects — the backend registry, IPC
threading, and frontend tab UI are tightly coupled (none is independently shippable; a
worker registry with no tab UI is unreachable, and a tab UI with no registry has nothing to
switch between). The natural task order is backend registry first (mechanical, testable in
isolation via the Rust test suite), then IPC/RepoClient threading, then `useOpenRepos` +
`RepoTabs`, then persistence, then the E2E multi-repo spec.
