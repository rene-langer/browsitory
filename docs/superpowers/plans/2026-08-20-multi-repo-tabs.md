# Multi-Repo Tabs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let Browsitory hold several repositories open at once, each with its own live backend worker and frontend state, switchable through a tab strip — with no reload on switch and the full tab set restored on next launch.

**Architecture:** The backend's single `Mutex<Option<Worker>>` becomes a `Mutex<HashMap<String, Worker>>` keyed by repo path, and every existing Tauri command gains a `repo_path` parameter to select which worker it targets. The frontend's single `useAppState` instance becomes one instance per open tab (each a normal, independently-mounted React component), coordinated by a new `useOpenRepos` hook that owns the tab list and persists it.

**Tech Stack:** Rust (Tauri, `git2`), TypeScript/React 19, Vitest, WebdriverIO.

**Spec:** `docs/superpowers/specs/2026-08-20-multi-repo-tabs-design.md`

## Global Constraints

- Repo identity is the raw path string as given to `open_repo` — no canonicalization, no new id scheme. This matches `crates/config/src/lib.rs`'s existing `recent_repos` list, which also stores and dedupes by the raw path with no canonicalization.
- Every existing single-repo feature must keep working unchanged per tab. Tasks that thread `repo_path`/`repoPath` through existing calls must not change any command's internal logic — only how its worker is selected.
- No new dependencies (Rust crates or npm packages).
- `VITE_E2E_REPO_PATH`'s auto-open behavior (`frontend/src/App.tsx`, E2E builds only) is unaffected — E2E fixtures need a deterministic single starting repo, not whatever tabs a prior run persisted.

---

## Task 1: Backend — persist the open-repo list

**Files:**
- Modify: `crates/config/src/lib.rs`
- Test: `crates/config/tests/` (existing integration test file for this crate — check its name with `ls crates/config/tests/` before adding; follow its existing `_at`-suffixed-function testing pattern)

**Interfaces:**
- Consumes: nothing new.
- Produces: `list_open_repos() -> Result<(Vec<PathBuf>, Option<PathBuf>), ConfigError>`, `set_open_repos(paths: &[PathBuf], active: Option<&Path>) -> Result<(), ConfigError>`, and their `_at`-suffixed testable variants — used by Task 2's new Tauri commands.

- [ ] **Step 1: Write the failing tests**

Add to the existing test file under `crates/config/tests/` (open it first and match its existing style — it already tests `list_recent_repos_at`/`add_recent_repo_at` against a temp file path):

```rust
#[test]
fn set_open_repos_at_persists_paths_and_active_repo() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    config::set_open_repos_at(
        &config_file,
        &[PathBuf::from("/repos/a"), PathBuf::from("/repos/b")],
        Some(&PathBuf::from("/repos/b")),
    )
    .unwrap();

    let (paths, active) = config::list_open_repos_at(&config_file).unwrap();
    assert_eq!(paths, vec![PathBuf::from("/repos/a"), PathBuf::from("/repos/b")]);
    assert_eq!(active, Some(PathBuf::from("/repos/b")));
}

#[test]
fn list_open_repos_at_on_a_missing_file_returns_empty() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    let (paths, active) = config::list_open_repos_at(&config_file).unwrap();
    assert!(paths.is_empty());
    assert_eq!(active, None);
}

#[test]
fn set_open_repos_at_does_not_disturb_recent_repos() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    config::add_recent_repo_at(&config_file, &PathBuf::from("/repos/recent")).unwrap();
    config::set_open_repos_at(&config_file, &[PathBuf::from("/repos/a")], None).unwrap();

    let recent = config::list_recent_repos_at(&config_file).unwrap();
    assert_eq!(recent, vec![PathBuf::from("/repos/recent")]);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p config` from the repo root.
Expected: FAIL to compile — `set_open_repos_at`/`list_open_repos_at` don't exist yet.

- [ ] **Step 3: Implement**

In `crates/config/src/lib.rs`, extend `ConfigFile` and add the four new functions, following the exact shape of the existing `recent_repos` functions immediately above them:

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

pub fn list_open_repos() -> Result<(Vec<PathBuf>, Option<PathBuf>), ConfigError> {
    list_open_repos_at(&config_file_path()?)
}

pub fn set_open_repos(paths: &[PathBuf], active: Option<&Path>) -> Result<(), ConfigError> {
    set_open_repos_at(&config_file_path()?, paths, active)
}

pub fn list_open_repos_at(config_file: &Path) -> Result<(Vec<PathBuf>, Option<PathBuf>), ConfigError> {
    let config = read_config(config_file)?;
    Ok((config.open_repos, config.active_repo))
}

pub fn set_open_repos_at(
    config_file: &Path,
    paths: &[PathBuf],
    active: Option<&Path>,
) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    config.open_repos = paths.to_vec();
    config.active_repo = active.map(|p| p.to_path_buf());
    write_config(config_file, &config)
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p config`
Expected: PASS, all tests including the three new ones and the pre-existing `recent_repos` ones.

- [ ] **Step 5: Commit**

```bash
git add crates/config/src/lib.rs crates/config/tests/
git commit -m "feat(config): persist the open-repo tab list and active repo"
```

---

## Task 2: Backend — worker registry and repo_path threading

**Files:**
- Modify: `crates/tauri-app/src/commands.rs`
- Modify: `crates/tauri-app/src/main.rs`
- Test: `crates/tauri-app/src/commands.rs` (its own `mod tests` block, extended)

**Interfaces:**
- Consumes: Task 1's `config::list_open_repos`/`config::set_open_repos`.
- Produces: `worker_handle(state, repo_path) -> Result<WorkerHandle, String>`; new commands `close_repo(repo_path, state)`, `list_open_repos()`, `persist_open_repos(paths, active_path)`. Every other existing command's signature gains a leading `repo_path: String` parameter — this is what Task 3 (the frontend `RepoClient`) calls with the tab's path.

### Step 1: Write the failing test

Add to `commands.rs`'s existing `mod tests` block (it currently only tests DTO serialization; this is its first test that spawns a real worker, so add the needed imports alongside the existing ones):

```rust
#[test]
fn two_open_repos_have_independent_worker_state() {
    use std::collections::HashMap;
    use crate::worker::Worker;

    let dir_a = tempfile::TempDir::new().unwrap();
    let repo_a = git2::Repository::init(dir_a.path()).unwrap();
    {
        let mut config = repo_a.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
    }
    std::fs::write(dir_a.path().join("a.txt"), "a").unwrap();

    let dir_b = tempfile::TempDir::new().unwrap();
    let repo_b = git2::Repository::init(dir_b.path()).unwrap();
    {
        let mut config = repo_b.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
    }

    let mut workers: HashMap<String, Worker> = HashMap::new();
    workers.insert(
        dir_a.path().to_string_lossy().into_owned(),
        Worker::spawn(dir_a.path().to_path_buf()).unwrap(),
    );
    workers.insert(
        dir_b.path().to_string_lossy().into_owned(),
        Worker::spawn(dir_b.path().to_path_buf()).unwrap(),
    );

    let handle_a = workers[&dir_a.path().to_string_lossy().into_owned()].handle();
    let handle_b = workers[&dir_b.path().to_string_lossy().into_owned()].handle();

    handle_a.stage_file("a.txt".to_string()).unwrap();

    assert_eq!(handle_a.get_status().unwrap().len(), 1);
    assert!(handle_a.get_status().unwrap()[0].staged);
    assert!(handle_b.get_status().unwrap().is_empty());
}
```

- [ ] Add this test now.

### Step 2: Run the test to verify it fails

Run: `cargo test -p tauri-app two_open_repos_have_independent_worker_state`
Expected: FAIL — either a compile error (if `Worker`/`stage_file` visibility doesn't reach the test) or, once it compiles, this test actually already passes today since it doesn't touch `AppState` at all yet. **This step exists to confirm the worker-level mechanics this task relies on are sound before wiring the registry around them** — if it fails to compile, fix visibility (`pub(crate)` on `Worker`'s fields/methods used here) before continuing, since Step 3 depends on the same APIs.

### Step 3: Implement the worker registry

Replace `AppState` and `worker_handle` in `commands.rs`:

```rust
#[derive(Default)]
pub struct AppState {
    pub workers: Mutex<HashMap<String, Worker>>,
}

fn worker_handle(state: &State<AppState>, repo_path: &str) -> Result<crate::worker::WorkerHandle, String> {
    let guard = state.workers.lock().unwrap_or_else(|e| e.into_inner());
    guard
        .get(repo_path)
        .map(Worker::handle)
        .ok_or_else(|| format!("repo not open: {repo_path}"))
}
```

Add `use std::collections::HashMap;` to this file's imports if not already present.

Replace `open_repo` and add `close_repo`:

```rust
#[tauri::command]
pub async fn open_repo(path: String, state: State<'_, AppState>) -> Result<(), String> {
    {
        let guard = state.workers.lock().unwrap_or_else(|e| e.into_inner());
        if guard.contains_key(&path) {
            drop(guard);
            let _ = config::add_recent_repo(Path::new(&path));
            return Ok(());
        }
    }
    let worker = Worker::spawn(PathBuf::from(&path))?;
    state
        .workers
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(path.clone(), worker);
    let _ = config::add_recent_repo(Path::new(&path));
    Ok(())
}

#[tauri::command]
pub async fn close_repo(repo_path: String, state: State<'_, AppState>) -> Result<(), String> {
    state
        .workers
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&repo_path);
    Ok(())
}

#[tauri::command]
pub fn list_open_repos() -> Result<(Vec<String>, Option<String>), String> {
    let (paths, active) = config::list_open_repos().map_err(|e| e.to_string())?;
    Ok((
        paths.into_iter().map(|p| p.to_string_lossy().into_owned()).collect(),
        active.map(|p| p.to_string_lossy().into_owned()),
    ))
}

#[tauri::command]
pub fn persist_open_repos(paths: Vec<String>, active_path: Option<String>) -> Result<(), String> {
    config::set_open_repos(
        &paths.into_iter().map(PathBuf::from).collect::<Vec<_>>(),
        active_path.as_deref().map(Path::new),
    )
    .map_err(|e| e.to_string())
}
```

### Step 4: Thread `repo_path` through every remaining command

**The rule:** every `#[tauri::command]` function below that takes `state: State<'_, AppState>` gets one new leading parameter, `repo_path: String`, and its call to `worker_handle(&state)` becomes `worker_handle(&state, &repo_path)`. Nothing else in the function body changes. `pick_repo_folder`, `list_recent_repos`, and `open_external_url` take no `state` and are excluded — leave them exactly as they are.

Four worked examples covering every parameter shape that occurs in the file:

```rust
// Before: no other params.
#[tauri::command]
pub async fn get_status(state: State<'_, AppState>) -> Result<Vec<StatusEntry>, String> {
    worker_handle(&state)?.get_status()
}
// After:
#[tauri::command]
pub async fn get_status(repo_path: String, state: State<'_, AppState>) -> Result<Vec<StatusEntry>, String> {
    worker_handle(&state, &repo_path)?.get_status()
}
```

```rust
// Before: one other param.
#[tauri::command]
pub async fn stage_file(path: String, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state)?.stage_file(path)
}
// After: repo_path goes first, ahead of the command's own params.
#[tauri::command]
pub async fn stage_file(repo_path: String, path: String, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.stage_file(path)
}
```

```rust
// Before: multiple other params.
#[tauri::command]
pub async fn create_branch(
    name: String,
    start_point: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state)?.create_branch(name, start_point)
}
// After:
#[tauri::command]
pub async fn create_branch(
    repo_path: String,
    name: String,
    start_point: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.create_branch(name, start_point)
}
```

```rust
// Before: a transfer command taking `app: AppHandle` too.
#[tauri::command]
pub async fn fetch_remote(
    remote_name: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let (event_tx, event_rx) = mpsc::channel();
    let operation_id = worker_handle(&state)?.fetch_remote(remote_name, event_tx)?;
    emit_transfer_events(app, event_rx);
    Ok(operation_id)
}
// After: repo_path still goes first; app/state ordering among themselves is unchanged.
#[tauri::command]
pub async fn fetch_remote(
    repo_path: String,
    remote_name: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let (event_tx, event_rx) = mpsc::channel();
    let operation_id = worker_handle(&state, &repo_path)?.fetch_remote(remote_name, event_tx)?;
    emit_transfer_events(app, event_rx);
    Ok(operation_id)
}
```

Apply the same rule to every one of these (name — current starting line in `commands.rs`, before this task's edits shift line numbers down):

`get_commit_graph` (746), `get_working_diff` (755), `get_commit_diff` (765), `get_commit_files` (775), `get_blame` (783), `stage_file` (793), `unstage_file` (798), `commit` (803), `list_branches` (808), `list_worktrees` (814), `create_worktree` (823), `remove_worktree` (834), `prune_worktrees` (839), `list_submodules` (844), `init_submodule` (853), `update_submodule` (858), `list_reflog_refs` (867), `get_reflog` (872), `restore_reflog_entry` (884), `create_branch` (893), `switch_branch` (902), `delete_branch` (907), `rename_branch` (916), `list_remotes` (925), `get_current_upstream` (938), `get_remote_upstreams` (945), `add_remote` (957), `rename_remote` (967), `update_remote_urls` (976), `remove_remote` (986), `save_https_credential` (995), `forget_https_credential` (1005), `set_remote_auth_mode` (1013), `set_current_upstream` (1031), `clear_current_upstream` (1040), `list_tags` (1045), `create_tag` (1054), `delete_tag` (1063), `fetch_remote` (1068), `push_current_branch` (1080), `push_tags` (1092), `pull_current_upstream` (1105), `list_stashes` (1121), `save_stash` (1127), `apply_stash` (1132), `drop_stash` (1137), `start_merge` (1142), `get_conflict_hunks` (1151), `resolve_conflict` (1160), `abort_merge` (1169), `get_merge_message` (1174), `resolve_add_delete_conflict` (1179), `commits_since` (1188), `start_rebase` (1197), `rebase_continue` (1208), `abort_rebase` (1214), `get_rebase_progress` (1219), `detect_forge_repository` (1232), `save_forge_token` (1243), `forget_forge_token` (1253), `list_pull_requests` (1262), `create_pull_request` (1273).

That's 63 functions. Go through them in file order; each is a one-line addition (`repo_path: String,` as the first parameter) plus one call-site edit (`worker_handle(&state)` → `worker_handle(&state, &repo_path)`).

### Step 5: Register the new commands in `main.rs`

Add `close_repo`, `list_open_repos`, `persist_open_repos` to both the `use commands::{...}` block and the `tauri::generate_handler![...]` list in `crates/tauri-app/src/main.rs`. Keep both lists alphabetically grouped the way the existing ones already are (`use` block is alphabetical; `generate_handler!` is not strictly alphabetical today — just append near related entries, e.g. put `close_repo` right after `open_repo`, and the two open-repos-list commands near `list_recent_repos`).

### Step 6: Run the tests to verify they pass

Run: `cargo build --workspace` (this is the real check for Step 4 — a missed call site or mismatched param order is a compile error, not a silently-wrong test) and then `cargo test -p tauri-app`.
Expected: both succeed; `two_open_repos_have_independent_worker_state` passes; every pre-existing test in `commands.rs` and `worker.rs` still passes unchanged (they test DTOs and `Worker`/`WorkerHandle` directly, never through the `#[tauri::command]` wrappers, so this task doesn't touch what they assert).

### Step 7: Commit

```bash
git add crates/tauri-app/src/commands.rs crates/tauri-app/src/main.rs
git commit -m "feat(tauri-app): replace the single-worker slot with a repo-keyed worker registry"
```

---

## Task 3: Frontend — thread `repoPath` through `RepoClient`

**Files:**
- Modify: `frontend/src/ipc/RepoClient.ts`
- Modify: `frontend/src/ipc/tauriRepoClient.ts`

**Interfaces:**
- Consumes: Task 2's Tauri commands (same names, snake_case on the Rust side, invoked here in camelCase per the existing `invoke("command_name", {...})` convention — Tauri maps `repo_path` ⇄ `repoPath` automatically, matching how every other argument already round-trips, e.g. `remoteName` ⇄ `remote_name`).
- Produces: the `repoPath`-taking `RepoClient` interface Task 4 (`useAppState`) and Task 5 (`useOpenRepos`) call against. New methods: `closeRepo(repoPath: string): Promise<void>`, `listOpenRepos(): Promise<{ paths: string[]; activePath: string | null }>`, `persistOpenRepos(paths: string[], activePath: string | null): Promise<void>`.

### Step 1: Update `RepoClient.ts`

**The rule:** every method in the `RepoClient` interface gains a leading `repoPath: string` parameter, **except** `pickRepoFolder`, `listRecentRepos`, `openRepo` (already takes `path` — that stays as-is, it *is* the repo path), `subscribeTransferProgress` (listens on a global, operation-id-filtered event stream — see the design spec's Testing section; adding `repoPath` here would do nothing, since the underlying Tauri event names aren't repo-scoped), and `openExternalUrl` (not repo-scoped — opens an arbitrary URL).

Two worked examples:

```typescript
// Before:
getStatus(): Promise<StatusEntry[]>;
// After:
getStatus(repoPath: string): Promise<StatusEntry[]>;
```

```typescript
// Before:
createBranch(name: string, startPoint: string): Promise<void>;
// After: repoPath goes first.
createBranch(repoPath: string, name: string, startPoint: string): Promise<void>;
```

Apply this to every method except the five named exclusions above — that's 63 of the 68 methods currently declared at lines 206–276 of `RepoClient.ts` (the same 63 that changed in Task 2, same names in camelCase: `getStatus`, `getCommitGraph`, `getWorkingDiff`, `getCommitDiff`, `getCommitFiles`, `stageFile`, `unstageFile`, `commit`, `listBranches`, `createBranch`, `switchBranch`, `deleteBranch`, `renameBranch`, `listWorktrees`, `createWorktree`, `removeWorktree`, `pruneWorktrees`, `listSubmodules`, `initSubmodule`, `updateSubmodule`, `listReflogRefs`, `getReflog`, `restoreReflogEntry`, `listRemotes`, `getCurrentUpstream`, `getRemoteUpstreams`, `addRemote`, `renameRemote`, `updateRemoteUrls`, `removeRemote`, `saveHttpsCredential`, `forgetHttpsCredential`, `setRemoteAuthMode`, `setCurrentUpstream`, `clearCurrentUpstream`, `listTags`, `createTag`, `deleteTag`, `fetchRemote`, `pushCurrentBranch`, `pushTags`, `pullCurrentUpstream`, `listStashes`, `saveStash`, `applyStash`, `dropStash`, `getBlame`, `mergeBranch`, `getConflictHunks`, `resolveConflict`, `abortMerge`, `getMergeMessage`, `resolveAddDeleteConflict`, `commitsSince`, `startRebase`, `rebaseContinue`, `abortRebase`, `getRebaseProgress`, `detectForgeRepository`, `saveForgeToken`, `forgetForgeToken`, `listPullRequests`, `createPullRequest`).

Then add the three new methods (place them near `openRepo`):

```typescript
closeRepo(repoPath: string): Promise<void>;
listOpenRepos(): Promise<{ paths: string[]; activePath: string | null }>;
persistOpenRepos(paths: string[], activePath: string | null): Promise<void>;
```

### Step 2: Update `tauriRepoClient.ts` to match

**The rule:** every implementation gains `repoPath: string` as its first parameter and passes it as the first key in the `invoke` payload object. Two worked examples matching Step 1's:

```typescript
// Before:
getStatus: () => invoke<StatusEntry[]>("get_status"),
// After:
getStatus: (repoPath: string) => invoke<StatusEntry[]>("get_status", { repoPath }),
```

```typescript
// Before:
createBranch: (name: string, startPoint: string) => invoke("create_branch", { name, startPoint }),
// After:
createBranch: (repoPath: string, name: string, startPoint: string) =>
  invoke("create_branch", { repoPath, name, startPoint }),
```

Apply to the same 63 methods (same list as Step 1), plus the three new ones:

```typescript
closeRepo: (repoPath: string) => invoke("close_repo", { repoPath }),
listOpenRepos: () =>
  invoke<[string[], string | null]>("list_open_repos").then(([paths, activePath]) => ({ paths, activePath })),
persistOpenRepos: (paths: string[], activePath: string | null) =>
  invoke("persist_open_repos", { paths, activePath }),
```

(`list_open_repos` returns a Rust tuple `(Vec<String>, Option<String>)`, which `serde` serializes as a JSON array of two elements — hence the `.then` reshaping into the named-fields shape the interface declares.)

### Step 3: Typecheck

Run: `cd frontend && npx tsc -b`
Expected: fails until every call site elsewhere in the frontend (App.tsx, useAppState.ts, RepoPicker.tsx, commands.ts) is updated — that's Tasks 4 and 7/8. **This task's own two files are internally consistent once Steps 1–2 are done**; the workspace-wide compile only goes green after Task 7. Confirm at least that `RepoClient.ts` and `tauriRepoClient.ts` type-check against each other with no mismatched signatures by checking the tsc output only reports errors in *other* files, not these two.

### Step 4: Commit

```bash
git add frontend/src/ipc/RepoClient.ts frontend/src/ipc/tauriRepoClient.ts
git commit -m "feat(frontend): thread repoPath through the RepoClient interface"
```

(This commit leaves the workspace non-compiling until Task 4 lands — acceptable mid-plan since a single engineer or agent runs these tasks back-to-back without pushing; if you're pairing this with the finishing-a-development-branch skill, don't run its test-suite gate until after Task 7.)

---

## Task 4: Frontend — parameterize `useAppState` by `repoPath`

**Files:**
- Modify: `frontend/src/state/useAppState.ts`
- Modify: `frontend/src/state/useAppState.test.ts`

**Interfaces:**
- Consumes: Task 3's `repoPath`-taking `RepoClient`.
- Produces: `useAppState(client: RepoClient, repoPath: string): UseAppStateResult` — note the new required second parameter, and that `openRepo` is **removed** from `UseAppStateResult` (opening/closing repos is now `useOpenRepos`'s job, Task 5 — by the time a `useAppState` instance exists, `useOpenRepos` has already ensured the backend worker for `repoPath` is open). `AppState.repoPath` changes from `string | null` to `string` (always the hook's own fixed `repoPath` argument, never null).

### Step 1: Update the hook signature and remove `openRepo`

In `useAppState.ts`:

- Change `export function useAppState(client: RepoClient): UseAppStateResult {` to `export function useAppState(client: RepoClient, repoPath: string): UseAppStateResult {`.
- Change the initial state's `repoPath: null` to `repoPath` (the argument, already in scope).
- Delete the `openRepo` callback (currently around line 316–332) entirely, and remove `openRepo` from the `UseAppStateResult` interface (around line 96).
- The transfer-subscription effect (around line 248–284) currently starts with `if (state.repoPath === null) return;` — since `repoPath` can no longer be null, delete that guard line, and change the effect's dependency array from `[client, refresh, state.repoPath]` to `[client, refresh, repoPath]` (the argument, not `state.repoPath` — both hold the same value now, but depending on the stable argument avoids a re-subscribe on every `setState` that happens to touch the `repoPath` field, which no longer varies anyway).

### Step 2: Thread `repoPath` into every `client.*` call

**The rule:** every `client.<method>(...)` call in this file becomes `client.<method>(repoPath, ...)`, for the same 63 methods Task 3 changed (this file only calls a subset of them — thread it wherever `client.` is called; leave calls to `client.pickRepoFolder`/`client.listRecentRepos`/`client.subscribeTransferProgress`/`client.openExternalUrl` untouched, and there should be no remaining call to `client.openRepo` after Step 1's deletion).

Two worked examples from this file:

```typescript
// Before:
client.getStatus(),
// After:
client.getStatus(repoPath),
```

```typescript
// Before:
await client.createBranch(name, startPoint);
// After:
await client.createBranch(repoPath, name, startPoint);
```

Apply this at every `client.` call site in the file (the grep for `client\.` in this file, excluding the four exclusions above, is the authoritative list — go through the file top to bottom).

### Step 3: Update `useAppState.test.ts`

Every `useAppState(client)` call in this test file becomes `useAppState(client, "/repo")` (or whatever placeholder path the surrounding test already uses for its mock — check for an existing constant; if none exists, introduce `const TEST_REPO_PATH = "/repo";` near the top of the file and use it everywhere). Every mock `RepoClient` method assertion that currently checks e.g. `expect(client.getStatus).toHaveBeenCalledWith()` becomes `expect(client.getStatus).toHaveBeenCalledWith(TEST_REPO_PATH)` (and similarly with real arguments appended after it, e.g. `toHaveBeenCalledWith(TEST_REPO_PATH, "main", "HEAD")`). Delete or move any test block specifically about the `openRepo` method — that behavior now belongs to `useOpenRepos.test.ts` (Task 5); if the design's coverage of "opening a repo persists it as recent" still needs a home, it moves there since `client.openRepo` is called from `useOpenRepos`, not `useAppState`, after this task.

### Step 4: Run the tests to verify they pass

Run: `cd frontend && npx vitest run useAppState.test.ts`
Expected: PASS. Then run `npx tsc -b` — this should now report errors **only** in `App.tsx`, `RepoPicker.tsx`'s caller, and `commands.ts` (Tasks 7/8), not in `useAppState.ts` or its test.

### Step 5: Commit

```bash
git add frontend/src/state/useAppState.ts frontend/src/state/useAppState.test.ts
git commit -m "feat(frontend): parameterize useAppState by repoPath, drop its openRepo method"
```

---

## Task 5: Frontend — `useOpenRepos` hook

**Files:**
- Create: `frontend/src/state/useOpenRepos.ts`
- Create: `frontend/src/state/useOpenRepos.test.ts`

**Interfaces:**
- Consumes: Task 3's `client.openRepo`, `client.closeRepo`, `client.listOpenRepos`, `client.persistOpenRepos`.
- Produces:

```typescript
export interface OpenRepo {
  path: string;
  displayName: string;
}

export interface UseOpenReposResult {
  openRepos: OpenRepo[];
  activePath: string | null;
  loading: boolean; // true until the initial listOpenRepos() restore completes
  openRepo(path: string): Promise<void>; // opens a new tab, or focuses it if already open
  closeRepo(path: string): void;
  switchTo(path: string): void;
}

export function useOpenRepos(client: RepoClient): UseOpenReposResult;
```

Consumed by Task 6 (`RepoTabs`), Task 7 (`App.tsx`), and Task 8 (`commands.ts`'s "Open worktree"/"Switch to repo" commands).

- [ ] **Step 1: Write the failing tests**

```typescript
import { describe, expect, it, vi } from "vitest";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { RepoClient } from "../ipc/RepoClient";
import { useOpenRepos } from "./useOpenRepos";

function fakeClient(overrides: Partial<RepoClient> = {}): RepoClient {
  return {
    openRepo: vi.fn().mockResolvedValue(undefined),
    closeRepo: vi.fn().mockResolvedValue(undefined),
    listOpenRepos: vi.fn().mockResolvedValue({ paths: [], activePath: null }),
    persistOpenRepos: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  } as unknown as RepoClient;
}

describe("useOpenRepos", () => {
  it("restores persisted tabs on mount, with the persisted active path focused", async () => {
    const client = fakeClient({
      listOpenRepos: vi.fn().mockResolvedValue({ paths: ["/repos/a", "/repos/b"], activePath: "/repos/b" }),
    });
    const { result } = renderHook(() => useOpenRepos(client));

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.openRepos.map((r) => r.path)).toEqual(["/repos/a", "/repos/b"]);
    expect(result.current.activePath).toBe("/repos/b");
  });

  it("opening a new path calls client.openRepo, adds a tab, and focuses it", async () => {
    const client = fakeClient();
    const { result } = renderHook(() => useOpenRepos(client));
    await waitFor(() => expect(result.current.loading).toBe(false));

    await act(async () => {
      await result.current.openRepo("/repos/new");
    });

    expect(client.openRepo).toHaveBeenCalledWith("/repos/new");
    expect(result.current.openRepos.map((r) => r.path)).toEqual(["/repos/new"]);
    expect(result.current.activePath).toBe("/repos/new");
    expect(client.persistOpenRepos).toHaveBeenLastCalledWith(["/repos/new"], "/repos/new");
  });

  it("opening an already-open path focuses it instead of duplicating the tab", async () => {
    const client = fakeClient({
      listOpenRepos: vi.fn().mockResolvedValue({ paths: ["/repos/a", "/repos/b"], activePath: "/repos/a" }),
    });
    const { result } = renderHook(() => useOpenRepos(client));
    await waitFor(() => expect(result.current.loading).toBe(false));

    await act(async () => {
      await result.current.openRepo("/repos/b");
    });

    expect(result.current.openRepos.map((r) => r.path)).toEqual(["/repos/a", "/repos/b"]);
    expect(result.current.activePath).toBe("/repos/b");
  });

  it("closing the active tab focuses the next tab, or the previous one if it was last", async () => {
    const client = fakeClient({
      listOpenRepos: vi.fn().mockResolvedValue({ paths: ["/repos/a", "/repos/b", "/repos/c"], activePath: "/repos/c" }),
    });
    const { result } = renderHook(() => useOpenRepos(client));
    await waitFor(() => expect(result.current.loading).toBe(false));

    act(() => result.current.closeRepo("/repos/c"));

    expect(client.closeRepo).toHaveBeenCalledWith("/repos/c");
    expect(result.current.openRepos.map((r) => r.path)).toEqual(["/repos/a", "/repos/b"]);
    expect(result.current.activePath).toBe("/repos/b");
  });

  it("derives displayName from the path's final segment", async () => {
    const client = fakeClient({
      listOpenRepos: vi.fn().mockResolvedValue({ paths: ["/repos/widget"], activePath: "/repos/widget" }),
    });
    const { result } = renderHook(() => useOpenRepos(client));
    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.openRepos[0].displayName).toBe("widget");
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd frontend && npx vitest run useOpenRepos.test.ts`
Expected: FAIL — `useOpenRepos` doesn't exist yet.

- [ ] **Step 3: Implement**

```typescript
import { useCallback, useEffect, useState } from "react";
import type { RepoClient } from "../ipc/RepoClient";

export interface OpenRepo {
  path: string;
  displayName: string;
}

export interface UseOpenReposResult {
  openRepos: OpenRepo[];
  activePath: string | null;
  loading: boolean;
  openRepo(path: string): Promise<void>;
  closeRepo(path: string): void;
  switchTo(path: string): void;
}

function displayNameFor(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const segments = trimmed.split(/[\\/]/);
  return segments[segments.length - 1] || trimmed;
}

export function useOpenRepos(client: RepoClient): UseOpenReposResult {
  const [openRepos, setOpenRepos] = useState<OpenRepo[]>([]);
  const [activePath, setActivePath] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let ignore = false;
    client.listOpenRepos().then(({ paths, activePath: restoredActive }) => {
      if (ignore) return;
      setOpenRepos(paths.map((path) => ({ path, displayName: displayNameFor(path) })));
      setActivePath(restoredActive ?? paths[0] ?? null);
      setLoading(false);
    });
    return () => {
      ignore = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const persist = useCallback(
    (repos: OpenRepo[], active: string | null) => {
      void client.persistOpenRepos(repos.map((r) => r.path), active);
    },
    [client],
  );

  const openRepo = useCallback(
    async (path: string) => {
      await client.openRepo(path);
      setOpenRepos((prev) => {
        const next = prev.some((r) => r.path === path)
          ? prev
          : [...prev, { path, displayName: displayNameFor(path) }];
        persist(next, path);
        return next;
      });
      setActivePath(path);
    },
    [client, persist],
  );

  const closeRepo = useCallback(
    (path: string) => {
      void client.closeRepo(path);
      setOpenRepos((prev) => {
        const closingIndex = prev.findIndex((r) => r.path === path);
        const next = prev.filter((r) => r.path !== path);
        setActivePath((prevActive) => {
          if (prevActive !== path) {
            persist(next, prevActive);
            return prevActive;
          }
          const nextActive = next[closingIndex]?.path ?? next[closingIndex - 1]?.path ?? null;
          persist(next, nextActive);
          return nextActive;
        });
        return next;
      });
    },
    [client, persist],
  );

  const switchTo = useCallback(
    (path: string) => {
      setActivePath(path);
      persist(openRepos, path);
    },
    [openRepos, persist],
  );

  return { openRepos, activePath, loading, openRepo, closeRepo, switchTo };
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd frontend && npx vitest run useOpenRepos.test.ts`
Expected: PASS, all 5 tests.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/state/useOpenRepos.ts frontend/src/state/useOpenRepos.test.ts
git commit -m "feat(frontend): add useOpenRepos hook for the tab list"
```

---

## Task 6: Frontend — `RepoTabs` component

**Files:**
- Create: `frontend/src/components/RepoTabs.tsx`
- Create: `frontend/src/components/RepoTabs.module.css`
- Create: `frontend/src/components/RepoTabs.test.tsx`

**Interfaces:**
- Consumes: `UseOpenReposResult` (Task 5) — specifically `openRepos`, `activePath`, `closeRepo`, `switchTo`; a separate `onAddTab: () => void` prop (App.tsx, Task 7, wires this to opening the `RepoPicker` overlay); and `busyPaths: ReadonlySet<string>` (App.tsx, Task 7 — the set of open repo paths whose tab currently has a transfer/merge/rebase in progress, per the design spec's "closing a tab with a transfer in flight" edge case). A path in `busyPaths` gets a disabled close button instead of an active one.
- Produces: `RepoTabs` component, mounted in `App.tsx`'s header.

- [ ] **Step 1: Write the failing tests**

```typescript
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { RepoTabs } from "./RepoTabs";

const repos = [
  { path: "/repos/widget", displayName: "widget" },
  { path: "/repos/gadget", displayName: "gadget" },
];

const noneBusy = new Set<string>();

describe("RepoTabs", () => {
  it("renders one tab per open repo, marking the active one", () => {
    render(
      <RepoTabs openRepos={repos} activePath="/repos/gadget" busyPaths={noneBusy} onSwitchTo={vi.fn()} onClose={vi.fn()} onAddTab={vi.fn()} />,
    );
    const active = screen.getByRole("tab", { name: /gadget/i, selected: true });
    expect(active).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /widget/i, selected: false })).toBeInTheDocument();
  });

  it("clicking a tab calls onSwitchTo with its path", () => {
    const onSwitchTo = vi.fn();
    render(<RepoTabs openRepos={repos} activePath="/repos/gadget" busyPaths={noneBusy} onSwitchTo={onSwitchTo} onClose={vi.fn()} onAddTab={vi.fn()} />);
    screen.getByRole("tab", { name: /widget/i }).click();
    expect(onSwitchTo).toHaveBeenCalledWith("/repos/widget");
  });

  it("clicking a tab's close control calls onClose with its path, not onSwitchTo", () => {
    const onClose = vi.fn();
    const onSwitchTo = vi.fn();
    render(<RepoTabs openRepos={repos} activePath="/repos/gadget" busyPaths={noneBusy} onSwitchTo={onSwitchTo} onClose={onClose} onAddTab={vi.fn()} />);
    screen.getByRole("button", { name: /close widget/i }).click();
    expect(onClose).toHaveBeenCalledWith("/repos/widget");
    expect(onSwitchTo).not.toHaveBeenCalled();
  });

  it("the trailing add button calls onAddTab", () => {
    const onAddTab = vi.fn();
    render(<RepoTabs openRepos={repos} activePath="/repos/gadget" busyPaths={noneBusy} onSwitchTo={vi.fn()} onClose={vi.fn()} onAddTab={onAddTab} />);
    screen.getByRole("button", { name: "Open another repository" }).click();
    expect(onAddTab).toHaveBeenCalled();
  });

  it("renders nothing when no repos are open", () => {
    const { container } = render(
      <RepoTabs openRepos={[]} activePath={null} busyPaths={noneBusy} onSwitchTo={vi.fn()} onClose={vi.fn()} onAddTab={vi.fn()} />,
    );
    expect(container.firstElementChild).toBeNull();
  });

  it("disables the close button for a tab in busyPaths, and clicking it does not call onClose", () => {
    const onClose = vi.fn();
    render(
      <RepoTabs
        openRepos={repos}
        activePath="/repos/gadget"
        busyPaths={new Set(["/repos/widget"])}
        onSwitchTo={vi.fn()}
        onClose={onClose}
        onAddTab={vi.fn()}
      />,
    );
    const closeButton = screen.getByRole("button", { name: /close widget/i });
    expect(closeButton).toBeDisabled();
    closeButton.click();
    expect(onClose).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd frontend && npx vitest run RepoTabs.test.tsx`
Expected: FAIL — `RepoTabs` doesn't exist yet.

- [ ] **Step 3: Implement**

`frontend/src/components/RepoTabs.tsx`:

```tsx
import { X, Plus } from "lucide-react";
import type { OpenRepo } from "../state/useOpenRepos";
import styles from "./RepoTabs.module.css";

export function RepoTabs({
  openRepos,
  activePath,
  busyPaths,
  onSwitchTo,
  onClose,
  onAddTab,
}: {
  openRepos: OpenRepo[];
  activePath: string | null;
  busyPaths: ReadonlySet<string>;
  onSwitchTo: (path: string) => void;
  onClose: (path: string) => void;
  onAddTab: () => void;
}) {
  if (openRepos.length === 0) return null;

  return (
    <div className={styles.tabs} role="tablist" aria-label="Open repositories">
      {openRepos.map((repo) => {
        const selected = repo.path === activePath;
        const busy = busyPaths.has(repo.path);
        return (
          <div key={repo.path} className={selected ? `${styles.tab} ${styles.active}` : styles.tab}>
            <button
              type="button"
              role="tab"
              aria-selected={selected}
              title={repo.path}
              className={styles.tabLabel}
              onClick={() => onSwitchTo(repo.path)}
            >
              {repo.displayName}
            </button>
            <button
              type="button"
              className={styles.closeButton}
              aria-label={`Close ${repo.displayName}`}
              title={busy ? "This repo has an operation in progress" : undefined}
              disabled={busy}
              onClick={() => onClose(repo.path)}
            >
              <X size={12} aria-hidden="true" />
            </button>
          </div>
        );
      })}
      <button type="button" className={styles.addButton} aria-label="Open another repository" onClick={onAddTab}>
        <Plus size={14} aria-hidden="true" />
      </button>
    </div>
  );
}
```

`frontend/src/components/RepoTabs.module.css` (token usage matches `App.module.css`'s existing `.headerRow`/`.themeToggle`):

```css
.tabs {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  flex: 1 1 auto;
  min-width: 0;
  overflow-x: auto;
  margin: 0 var(--space-4);
}

.tab {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  padding: var(--space-1) var(--space-2);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  background: var(--color-bg);
  flex: 0 0 auto;
  max-width: 16rem;
}

.tab.active {
  background: var(--color-bg-subtle);
  border-color: var(--color-accent, var(--color-border));
}

.tabLabel {
  background: none;
  border: none;
  color: var(--color-text);
  cursor: pointer;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.closeButton,
.addButton {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: none;
  background: none;
  color: var(--color-text-subtle, var(--color-text));
  cursor: pointer;
  padding: var(--space-1);
  border-radius: var(--radius-sm);
}

.closeButton:hover,
.addButton:hover {
  background: var(--color-bg-subtle);
}
```

Before using `--color-accent`/`--color-text-subtle`, check `frontend/src/styles/` (or wherever this project's CSS custom properties are defined — search for `--color-border:` to find the token file) for whether those two tokens already exist; if not, drop the `var(--color-accent, ...)`/`var(--color-text-subtle, ...)` fallback syntax and just use `var(--color-border)`/`var(--color-text)` directly instead of introducing new tokens.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd frontend && npx vitest run RepoTabs.test.tsx`
Expected: PASS, all 6 tests.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/RepoTabs.tsx frontend/src/components/RepoTabs.module.css frontend/src/components/RepoTabs.test.tsx
git commit -m "feat(frontend): add RepoTabs tab strip component"
```

---

## Task 7: Frontend — restructure `App.tsx` around multiple tabs

**Files:**
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/App.module.css`

**Interfaces:**
- Consumes: `useOpenRepos` (Task 5), `RepoTabs` (Task 6), `useAppState(client, repoPath)` (Task 4).
- Produces: a new `RepoWorkspace` component (defined in this same file — it's the old `App`'s three-column-layout body, now parameterized by `repoPath` and an `active` flag) rendered once per open tab. `RepoWorkspace` also takes an `onBusyChange: (repoPath: string, busy: boolean) => void` prop, called whenever its own `repositoryOperationDisabled` changes, so `App` can build the `busyPaths` set `RepoTabs` (Task 6) needs for its close-button guard.

### Step 1: Extract the existing three-column layout into `RepoWorkspace`

Everything in today's `App.tsx` from `const [paletteOpen, setPaletteOpen] = useState(false);` down through the closing `</main>` (i.e. everything that assumes one `appState`) moves into a new component:

```tsx
function RepoWorkspace({
  repoPath,
  active,
  onOpenRepoTab,
  onBusyChange,
}: {
  repoPath: string;
  active: boolean;
  onOpenRepoTab: (path: string) => void;
  onBusyChange: (repoPath: string, busy: boolean) => void;
}) {
  const appState = useAppState(tauriRepoClient, repoPath);
  const [paletteOpen, setPaletteOpen] = useState(false);

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (!active) return;
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setPaletteOpen((prev) => !prev);
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [active]);

  const repositoryOperationDisabled =
    appState.state.pending ||
    appState.state.transfer !== null ||
    appState.state.mergeMessage !== null ||
    appState.state.rebaseProgress !== null;

  // Closing this tab while a transfer/merge/rebase is in progress would orphan it mid-operation
  // — report busy status up so `App`'s `RepoTabs` can disable this tab's close button, the same
  // rule that already disables every other mutating action while this is true.
  useEffect(() => {
    onBusyChange(repoPath, repositoryOperationDisabled);
  }, [repoPath, repositoryOperationDisabled, onBusyChange]);

  return (
    <div style={{ display: active ? "contents" : "none" }}>
      {appState.state.error !== null && <p role="alert">{appState.state.error}</p>}
      {appState.state.transfer !== null && (
        <Overlay>
          <TransferPanel progress={appState.state.transfer} />
        </Overlay>
      )}
      {paletteOpen && (
        <Overlay onClose={() => setPaletteOpen(false)}>
          <CommandPalette commands={buildCommands(appState, onOpenRepoTab)} onRun={() => setPaletteOpen(false)} />
        </Overlay>
      )}
      <SplitView
        storageKey="sidebar-width"
        defaultWidth={260}
        minWidth={200}
        maxWidth={420}
        collapsible
        label="Sidebar width"
        left={
          <Sidebar>
            <BranchSwitcher
              branches={appState.state.branches}
              createBranchDraft={appState.state.createBranchDraft}
              onSwitchBranch={appState.switchBranch}
              onCreateBranch={appState.createBranch}
              onDeleteBranch={appState.deleteBranch}
              onRenameBranch={appState.renameBranch}
              onOpenCreateBranchDraft={appState.openCreateBranchDraft}
              onCloseCreateBranchDraft={appState.closeCreateBranchDraft}
              onMergeBranch={appState.mergeBranch}
              isMerging={appState.state.mergeMessage !== null}
              isRebasing={appState.state.rebaseProgress !== null}
              operationDisabled={repositoryOperationDisabled}
            />
            <WorktreePanel
              worktrees={appState.state.worktrees}
              branches={appState.state.branches}
              onOpenWorktree={onOpenRepoTab}
              onCreateWorktree={appState.createWorktree}
              onRemoveWorktree={appState.removeWorktree}
              onPruneWorktrees={appState.pruneWorktrees}
              operationDisabled={repositoryOperationDisabled}
            />
            <SubmodulePanel
              submodules={appState.state.submodules}
              onInit={appState.initSubmodule}
              onUpdate={appState.updateSubmodule}
              operationDisabled={repositoryOperationDisabled}
            />
            <ReflogPanel
              references={appState.state.reflogRefs}
              selectedReference={appState.state.selectedReflogReference}
              entries={appState.state.reflog}
              onSelectReference={appState.selectReflogReference}
              onRestore={appState.restoreReflogEntry}
              operationDisabled={repositoryOperationDisabled}
            />
            <RemotePanel
              remotes={appState.state.remotes}
              upstream={appState.state.upstream}
              remoteUpstreams={appState.state.remoteUpstreams}
              onAddRemote={appState.addRemote}
              onRenameRemote={appState.renameRemote}
              onUpdateRemoteUrls={appState.updateRemoteUrls}
              onRemoveRemote={appState.removeRemote}
              onSaveHttpsCredential={appState.saveHttpsCredential}
              onForgetHttpsCredential={appState.forgetHttpsCredential}
              onSetRemoteAuthMode={appState.setRemoteAuthMode}
              onSetUpstream={appState.setCurrentUpstream}
              onClearUpstream={appState.clearCurrentUpstream}
              onFetchRemote={appState.fetchRemote}
              fetchDisabled={repositoryOperationDisabled}
              onPushCurrentBranch={appState.pushCurrentBranch}
              pushDisabled={repositoryOperationDisabled}
              onPull={appState.pullCurrentUpstream}
              pullDisabled={repositoryOperationDisabled}
              pendingPull={appState.state.pendingPull}
              pullOutcome={appState.state.pullOutcome}
              onMergePull={async (upstreamRef) => {
                appState.clearPendingPull();
                await appState.mergeBranch(upstreamRef);
              }}
              onRebasePull={(upstreamRef) => {
                appState.clearPendingPull();
                appState.openRebasePlanner(upstreamRef);
              }}
              onCancelPull={appState.clearPendingPull}
            />
            <TagPanel
              tags={appState.state.tags}
              remotes={appState.state.remotes}
              onCreate={appState.createTag}
              onDelete={appState.deleteTag}
              onPush={appState.pushTags}
              pushDisabled={repositoryOperationDisabled}
            />
            <PullRequestPanel
              forgeRepositories={appState.state.forgeRepositories}
              pullRequests={appState.state.pullRequests}
              onListPullRequests={appState.listPullRequests}
              onSaveForgeToken={appState.saveForgeToken}
              onForgetForgeToken={appState.forgetForgeToken}
              onCreatePullRequest={appState.createPullRequest}
              onOpenExternalUrl={appState.openExternalUrl}
              operationDisabled={repositoryOperationDisabled}
            />
          </Sidebar>
        }
        right={
          <SplitView
            storageKey="history-diff-width"
            defaultWidth={420}
            minWidth={280}
            maxWidth={800}
            label="History and diff width"
            left={
              <CommitGraph
                status={appState.state.status}
                commits={appState.state.commits}
                stashes={appState.state.stashes}
                selectedRow={appState.state.selectedRow}
                pending={repositoryOperationDisabled}
                onSelectRow={appState.selectRow}
                onBranchFromCommit={appState.openCreateBranchDraft}
                onRebaseFromCommit={appState.openRebasePlanner}
                onApplyStash={appState.applyStash}
                onDropStash={appState.dropStash}
              />
            }
            right={
              <DiffPane
                client={tauriRepoClient}
                selectedRow={appState.state.selectedRow}
                status={appState.state.status}
                onStageFile={appState.stageFile}
                onUnstageFile={appState.unstageFile}
                onCommit={appState.commit}
                onSaveStash={appState.saveStash}
                onSelectRow={appState.selectRow}
                onResolveConflict={appState.resolveConflict}
                onResolveAddDeleteConflict={appState.resolveAddDeleteConflict}
                mergeMessage={appState.state.mergeMessage}
                onAbortMerge={appState.abortMerge}
                rebaseProgress={appState.state.rebaseProgress}
                onRebaseContinue={appState.rebaseContinue}
                onRebaseAbort={appState.abortRebase}
              />
            }
          />
        }
      />
      {appState.state.rebaseOnto !== null && (
        <Overlay onClose={appState.closeRebasePlanner}>
          <RebasePlanner
            client={tauriRepoClient}
            onto={appState.state.rebaseOnto}
            onStartRebase={appState.startRebase}
            onCancel={appState.closeRebasePlanner}
            operationDisabled={repositoryOperationDisabled}
          />
        </Overlay>
      )}
    </div>
  );
}
```

Notes on what changed from the original body: the Ctrl/Cmd+K handler now checks `active` instead of `appState.state.repoPath === null` (a background tab must not react to the shortcut — only the visible one should); the whole returned tree is wrapped in a `div` toggling `display: contents` (visible, lets its children participate in `App`'s layout normally) vs `display: none` (hidden but still mounted, so its `useAppState` instance and any in-flight IPC calls survive) based on `active`; `WorktreePanel`'s `onOpenWorktree` now takes `onOpenRepoTab` (opens the worktree as a new tab) instead of the old `appState.openRepo`; `buildCommands` now takes a second argument, `onOpenRepoTab` (Task 8 changes its signature to match).

### Step 2: Rewrite the top-level `App` component

`App` owns three things `RepoWorkspace` can't: the tab list itself (`useOpenRepos`), the "open another repo" overlay (a second `RepoPicker` instance, separate from the one shown when zero repos are open), and the aggregated `busyPaths` set each `RepoWorkspace` reports into via `onBusyChange`.

```tsx
export default function App() {
  const openRepos = useOpenRepos(tauriRepoClient);
  const [theme, setTheme] = useState<Theme>(() =>
    resolveTheme(
      loadStoredTheme(),
      window.matchMedia("(prefers-color-scheme: dark)").matches,
    ),
  );
  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  const [busyByPath, setBusyByPath] = useState<Record<string, boolean>>({});
  const onBusyChange = useCallback((repoPath: string, busy: boolean) => {
    setBusyByPath((prev) => (prev[repoPath] === busy ? prev : { ...prev, [repoPath]: busy }));
  }, []);
  const busyPaths = useMemo(
    () => new Set(Object.entries(busyByPath).filter(([, busy]) => busy).map(([path]) => path)),
    [busyByPath],
  );

  const [pickingRepo, setPickingRepo] = useState(false);

  const themeToggle = (
    <button
      type="button"
      className={styles.themeToggle}
      aria-label={theme === "dark" ? "Switch to light theme" : "Switch to dark theme"}
      onClick={() => {
        const next = theme === "dark" ? "light" : "dark";
        setTheme(next);
        persistTheme(next);
      }}
    >
      {theme === "dark" ? <Sun size={16} /> : <Moon size={16} />}
    </button>
  );

  // E2E-only auto-open: RepoPicker's native folder dialog can't be driven through WebDriver
  // (see App.tsx's original comment on this, carried over unchanged), so the E2E build points
  // at a fixture repo via this Vite env var instead — opened as this session's first tab.
  useEffect(() => {
    const autoOpenPath = import.meta.env.VITE_E2E_REPO_PATH;
    if (typeof autoOpenPath === "string" && autoOpenPath.length > 0 && openRepos.openRepos.length === 0) {
      void openRepos.openRepo(autoOpenPath);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [openRepos.loading]);

  if (openRepos.loading) {
    return null;
  }

  return (
    <main>
      <header className={styles.headerRow}>
        <h1>Browsitory</h1>
        <RepoTabs
          openRepos={openRepos.openRepos}
          activePath={openRepos.activePath}
          busyPaths={busyPaths}
          onSwitchTo={openRepos.switchTo}
          onClose={openRepos.closeRepo}
          onAddTab={() => setPickingRepo(true)}
        />
        {themeToggle}
      </header>
      <LaneBraid />
      {pickingRepo && (
        <Overlay onClose={() => setPickingRepo(false)}>
          <RepoPicker
            client={tauriRepoClient}
            onOpenRepo={(path) => {
              void openRepos.openRepo(path);
              setPickingRepo(false);
            }}
          />
        </Overlay>
      )}
      {openRepos.openRepos.length === 0 ? (
        <RepoPicker client={tauriRepoClient} onOpenRepo={openRepos.openRepo} />
      ) : (
        openRepos.openRepos.map((repo) => (
          <RepoWorkspace
            key={repo.path}
            repoPath={repo.path}
            active={repo.path === openRepos.activePath}
            onOpenRepoTab={openRepos.openRepo}
            onBusyChange={onBusyChange}
          />
        ))
      )}
    </main>
  );
}
```

Add `Overlay` to this file's imports (it's currently only imported inside the old body, which moved into `RepoWorkspace` in Step 1 — `App` needs its own import: `import { Overlay } from "./components/primitives/Overlay";`), and add `useCallback`/`useMemo` to the existing `import { useEffect, useState } from "react";` line.

### Step 3: Typecheck and run the full frontend suite

Run: `cd frontend && npx tsc -b && npx vitest run`
Expected: both succeed. If `tsc` reports leftover errors, they're in `commands.ts` (Task 8) — `buildCommands`'s call site here already passes the new second argument; `commands.ts` itself is updated next.

### Step 4: Commit

```bash
git add frontend/src/App.tsx frontend/src/App.module.css
git commit -m "feat(frontend): restructure App.tsx around multiple repo tabs"
```

---

## Task 8: Frontend — command palette repo commands

**Files:**
- Modify: `frontend/src/lib/commands.ts`
- Modify: `frontend/src/lib/commands.test.ts`

**Interfaces:**
- Consumes: `UseOpenReposResult`'s `openRepos`/`switchTo` (Task 5), passed in from `App.tsx`/`RepoWorkspace` (Task 7) — but `RepoWorkspace` only has `onOpenRepoTab`, not the full tab list or `switchTo`. This task needs `RepoWorkspace` to also receive `openRepos: OpenRepo[]` and `onSwitchRepoTab: (path: string) => void` — go back and add those two props to `RepoWorkspace` (Task 7) and thread them from `App.tsx`'s `<RepoWorkspace ... openRepos={openRepos.openRepos} onSwitchRepoTab={openRepos.switchTo} />`.
- Produces: `buildCommands(appState: UseAppStateResult, onOpenRepoTab: (path: string) => void, otherOpenRepos: OpenRepo[], onSwitchRepoTab: (path: string) => void): Command[]` — note the signature grows by three parameters total (this task adds two more beyond Task 7's `onOpenRepoTab`, which Task 7 already wired assuming this exact final shape... go back and update `RepoWorkspace`'s call `buildCommands(appState, onOpenRepoTab)` to `buildCommands(appState, onOpenRepoTab, otherOpenRepos, onSwitchRepoTab)`).

### Step 1: Write the failing tests

Add to `commands.test.ts` (open it first to match its existing test setup for constructing a fake `UseAppStateResult` — reuse whatever helper it already has):

```typescript
it("includes a Switch to <repo> command for every other open repo", () => {
  const appState = fakeAppState(); // however this file's existing tests construct one
  const otherRepos = [
    { path: "/repos/widget", displayName: "widget" },
    { path: "/repos/gadget", displayName: "gadget" },
  ];
  const onSwitchRepoTab = vi.fn();
  const commands = buildCommands(appState, vi.fn(), otherRepos, onSwitchRepoTab);

  const widgetCommand = commands.find((c) => c.id === "switch-repo:/repos/widget");
  expect(widgetCommand?.label).toBe("Switch to widget");
  widgetCommand?.run();
  expect(onSwitchRepoTab).toHaveBeenCalledWith("/repos/widget");
});

it("opening a worktree calls onOpenRepoTab instead of a state mutation", () => {
  const appState = fakeAppState({
    worktrees: [{ name: "feature", path: "/repos/feature-wt", head: "abc123", isMain: false, isLocked: false, isPrunable: false }],
  });
  const onOpenRepoTab = vi.fn();
  const commands = buildCommands(appState, onOpenRepoTab, [], vi.fn());

  const openCommand = commands.find((c) => c.id === "open-worktree:/repos/feature-wt");
  openCommand?.run();
  expect(onOpenRepoTab).toHaveBeenCalledWith("/repos/feature-wt");
});
```

(Adjust `fakeAppState`'s exact name/shape to match whatever helper `commands.test.ts` already uses — read the file first; do not invent a second helper if one exists.)

### Step 2: Run the tests to verify they fail

Run: `cd frontend && npx vitest run commands.test.ts`
Expected: FAIL — `buildCommands` doesn't accept these new parameters yet, and there's no `switch-repo:` command.

### Step 3: Implement

In `commands.ts`:

- Change the signature: `export function buildCommands(appState: UseAppStateResult, onOpenRepoTab: (path: string) => void, otherOpenRepos: OpenRepo[], onSwitchRepoTab: (path: string) => void): Command[] {` — add `import type { OpenRepo } from "../state/useOpenRepos";` at the top.
- Change the existing worktree-open command's `run` from `run: () => void appState.openRepo(worktree.path),` to `run: () => onOpenRepoTab(worktree.path),`.
- Add, right after the `SIDEBAR_SECTIONS` navigation loop (near the end of `buildCommands`, before `return commands;`):

```typescript
for (const repo of otherOpenRepos) {
  commands.push({
    id: `switch-repo:${repo.path}`,
    label: `Switch to ${repo.displayName}`,
    keywords: ["repo", "switch", "tab", repo.displayName],
    run: () => onSwitchRepoTab(repo.path),
  });
}
```

### Step 4: Run the tests to verify they pass

Run: `cd frontend && npx vitest run commands.test.ts`
Expected: PASS.

### Step 5: Fix `RepoWorkspace`'s call site (from Task 7) and typecheck the whole frontend

Back in `App.tsx`, update `RepoWorkspace`'s props and its `buildCommands` call as described in this task's Interfaces section above. Then:

Run: `cd frontend && npx tsc -b && npx vitest run`
Expected: both succeed with zero errors — this is the first point in the plan where the whole frontend compiles and every test passes.

### Step 6: Commit

```bash
git add frontend/src/lib/commands.ts frontend/src/lib/commands.test.ts frontend/src/App.tsx
git commit -m "feat(frontend): add repo-switch commands to the palette, route worktree-open through tabs"
```

---

## Task 9: E2E — multi-repo spec

**Files:**
- Modify: `crates/config/src/lib.rs`
- Modify: `e2e/wdio.conf.ts`
- Create: `e2e/specs/multi-repo.spec.ts`

**Interfaces:**
- Consumes: everything above, end to end.

### Step 1: Add a config-dir override to the `config` crate

`RepoPicker`'s "Open Folder" button drives a native OS file dialog, which WebDriver can't operate — the existing suite already works around this for the *first* repo via `VITE_E2E_REPO_PATH`'s baked-in auto-open (see `wdio.conf.ts`'s header comment). This spec needs a *second* repo to already be in `RepoPicker`'s recent-repos list so it can click that row instead of using the folder dialog — which means seeding `crates/config`'s `config.toml` from outside the app before it launches. That file's location today is hardcoded to the OS config dir (`directories::ProjectDirs`), which E2E has no way to reach or override. Add one:

In `crates/config/src/lib.rs`, change `config_file_path`:

```rust
fn config_file_path() -> Result<PathBuf, ConfigError> {
    if let Ok(dir) = std::env::var("BROWSITORY_CONFIG_DIR") {
        return Ok(PathBuf::from(dir).join("config.toml"));
    }
    let dirs = directories::ProjectDirs::from("com", "browsitory", "Browsitory")
        .ok_or(ConfigError::NoConfigDir)?;
    Ok(dirs.config_dir().join("config.toml"))
}
```

Add a test for the override alongside this crate's existing tests in `crates/config/tests/` (match its existing style and imports):

```rust
#[test]
fn config_file_path_env_override_is_used_when_set() {
    let dir = tempfile::TempDir::new().unwrap();
    // SAFETY (test-only): this crate's tests don't run this one in parallel with another
    // that also touches BROWSITORY_CONFIG_DIR — it's set and cleared within this single test.
    std::env::set_var("BROWSITORY_CONFIG_DIR", dir.path());
    let result = (|| -> Result<(), config::ConfigError> {
        config::add_recent_repo(std::path::Path::new("/repos/env-override-check"))?;
        let recent = config::list_recent_repos()?;
        assert_eq!(recent, vec![std::path::PathBuf::from("/repos/env-override-check")]);
        assert!(dir.path().join("config.toml").exists());
        Ok(())
    })();
    std::env::remove_var("BROWSITORY_CONFIG_DIR");
    result.unwrap();
}
```

Run `cargo test -p config` to confirm it passes, then commit this amendment on its own:

```bash
git add crates/config/src/lib.rs crates/config/tests/
git commit -m "feat(config): add a BROWSITORY_CONFIG_DIR override for E2E"
```

### Step 2: Add a second fixture repo, and seed it into the recent-repos config

`wdio.conf.ts`'s `onPrepare` already builds `E2E_REPO_PATH` (via `resetFixtureRepo`, cloned from `E2E_PARENT_SOURCE_PATH`). This spec needs a second, genuinely independent fixture repo (not a branch of the same one, since the point is to prove cross-repo isolation), and that second repo's path needs to already be in `RepoPicker`'s recent-repos list by the time the app launches — `RepoPicker`'s "Open Folder" button drives a native OS dialog WebDriver can't operate, so the spec opens the second repo by clicking its already-listed recent-repo row instead.

Add near the top of `wdio.conf.ts`, alongside the other path constants:

```typescript
const E2E_SECOND_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-second-repo");
const E2E_CONFIG_DIR = path.join(os.tmpdir(), "browsitory-e2e-config");
```

In `onPrepare`, after the existing `setupFixtureRepo(E2E_PARENT_SOURCE_PATH);` / `setupSubmoduleFixture(...)` / `resetFixtureRepo();` / `setupCredentialCertificate();` calls, add:

```typescript
setupFixtureRepo(E2E_SECOND_REPO_PATH);
fs.writeFileSync(path.join(E2E_SECOND_REPO_PATH, "second.txt"), "second repo\n");
execFileSync("git", ["add", "second.txt"], { cwd: E2E_SECOND_REPO_PATH, stdio: "inherit" });
execFileSync("git", ["commit", "-m", "e2e: second repo base commit"], { cwd: E2E_SECOND_REPO_PATH, stdio: "inherit" });

fs.rmSync(E2E_CONFIG_DIR, { recursive: true, force: true });
fs.mkdirSync(E2E_CONFIG_DIR, { recursive: true });
fs.writeFileSync(
  path.join(E2E_CONFIG_DIR, "config.toml"),
  `recent_repos = ["${E2E_SECOND_REPO_PATH.replace(/\\/g, "\\\\")}"]\n`,
);
process.env.BROWSITORY_CONFIG_DIR = E2E_CONFIG_DIR;
```

(`setupFixtureRepo` already exists in this file — `git init` + a local identity + an uncommitted `README.md`; reused here for a second, independent path, then given one real commit so it has a resolvable `HEAD` immediately, unlike the primary fixture, which relies on `first-flow.spec.ts` to make its first commit. `BROWSITORY_CONFIG_DIR` must be set in `onPrepare`, before `beforeSession` spawns `tauri-driver` — which spawns the app — so the app inherits it, the same timing this file already relies on for `SSL_CERT_FILE`/`BROWSITORY_FORGE_GITHUB_API_BASE_URL`.)

Neither the second repo nor the config dir need a `resetFixtureRepo`-style per-test reset — both are used only by this one spec, and `beforeTest`'s `browser.refresh()` doesn't touch on-disk state.

### Step 3: Write the spec

```typescript
import path from "node:path";
import os from "node:os";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const E2E_SECOND_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-second-repo");

async function waitForAppReady(): Promise<void> {
  await $('section[aria-label="Branches"] button[aria-expanded]').waitForExist({ timeout: 10000 });
}

describe("Browsitory multi-repo tabs", () => {
  it("opens a second, independent repo as a new tab, switches, and isolates per-tab state", async () => {
    await waitForAppReady();

    await browser.execute((el) => (el as HTMLElement).click(), await $('button[aria-label="Open another repository"]'));

    const secondRepoRow = await $(`li*=${E2E_SECOND_REPO_PATH}`);
    await secondRepoRow.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), secondRepoRow);

    await browser.waitUntil(
      async () => (await $$('[role="tab"]')).length === 2,
      { timeout: 10000, timeoutMsg: "expected a second tab after opening the second repo" },
    );

    const secondTab = await $(`button[title="${E2E_SECOND_REPO_PATH}"]`);
    await secondTab.waitForExist({ timeout: 10000 });
    expect(await secondTab.getAttribute("aria-selected")).toBe("true");

    const secondRepoCommit = await $("li*=e2e: second repo base commit");
    await secondRepoCommit.waitForExist({ timeout: 10000 });

    // Switch back to the first tab and confirm the second repo's commit isn't visible there —
    // proves per-tab state isolation, not just that two tabs exist.
    const firstTab = await $(`button[title="${E2E_REPO_PATH}"]`);
    await browser.execute((el) => (el as HTMLElement).click(), firstTab);
    await browser.waitUntil(
      async () => (await firstTab.getAttribute("aria-selected")) === "true",
      { timeout: 10000, timeoutMsg: "expected switching back to focus the first tab" },
    );
    const secondRepoCommitFromFirstTab = await $("li*=e2e: second repo base commit");
    expect(await secondRepoCommitFromFirstTab.isExisting()).toBe(false);

    await browser.execute(
      (el) => (el as HTMLElement).click(),
      await $(`button[aria-label="Close ${path.basename(E2E_SECOND_REPO_PATH)}"]`),
    );
    await browser.waitUntil(
      async () => (await $$('[role="tab"]')).length === 1,
      { timeout: 10000, timeoutMsg: "expected the second tab to close" },
    );
  });
});
```

### Step 4: Run the spec to verify it fails, then passes

Build first (this spec depends on every earlier task's backend and frontend changes):

```bash
cd frontend && VITE_E2E_REPO_PATH="$(node -e "console.log(require('os').tmpdir())")/browsitory-e2e-repo" npx vite build
cd .. && cargo build --workspace --features tauri-app/custom-protocol,tauri-app/forge-fixture-override
```

Run: `cd e2e && xvfb-run --auto-servernum npx wdio run wdio.conf.ts --spec ./specs/multi-repo.spec.ts`
Expected: FAILs if run before Tasks 1–8 are done (or before Step 1's `config` crate change lands); PASSes once everything above is in place.

### Step 5: Run the full E2E suite to confirm no regressions

Run: `cd e2e && xvfb-run --auto-servernum npx wdio run wdio.conf.ts`
Expected: every existing spec still passes — each opens exactly one repo, so the `BROWSITORY_CONFIG_DIR` override and second fixture repo this task adds are inert for them (they never look at `recent_repos` or a second path).

### Step 6: Commit

```bash
git add e2e/wdio.conf.ts e2e/specs/multi-repo.spec.ts
git commit -m "test(e2e): add multi-repo tab spec"
```

---

## Final Verification

After all 9 tasks:

- [ ] `cargo test --workspace` passes.
- [ ] `cd frontend && npx tsc -b && npx eslint . && npx vitest run` all pass.
- [ ] `cd e2e && xvfb-run --auto-servernum npx wdio run wdio.conf.ts` passes in full (not just `multi-repo.spec.ts`).
- [ ] Manually verify (or have the final review confirm via the E2E harness) that closing the last tab returns to the `RepoPicker` screen, and that relaunching the built app restores every tab that was open, with the previously-active one focused.
