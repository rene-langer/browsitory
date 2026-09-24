# Remote Branch Labels in Commit Graph Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show remote-tracking branch tips (`origin/main`, …) as badges on commits in `CommitGraph`, alongside the existing local branch badges, with their own persisted show/hide toggle in `BranchTree`.

**Architecture:** `git-core::graph::graph_log` gains a second per-commit field (`remote_branch_refs`) populated the same way as the existing `branch_refs`, from `refs/remotes/*` tips instead of `refs/heads/*`. It flows unchanged through `repo-service` (which returns the domain `GraphCommit` type directly) to two DTO/command layers (`tauri-app`, `vscode-sidecar`) and into the frontend `RepoClient` contract. A new, independent persisted selection (mirroring the existing local `graphBranchSelection`) controls which badges `CommitGraph` renders — it never affects `graph_log`'s revwalk, so it's applied client-side only.

**Tech Stack:** Rust (`git2`, `serde`, `toml`), React/TypeScript, Vitest, cargo test.

**Spec:** `docs/superpowers/specs/2026-09-24-remote-branch-graph-labels-design.md`

## Global Constraints

- Labels-only scope: no revwalk change. A commit only gets a remote badge if it's already reachable from a local branch (existing `graph_log` behavior, unchanged).
- Remote badge selection state is independent of local `graphBranchSelection` — it only filters which badges `CommitGraph` renders, never which commits are fetched.
- Qualified remote branch name form throughout (`"origin/main"`), matching `git2`'s `Branch::name()` — no separate remote/branch-name splitting anywhere in this feature.
- `origin/HEAD` and other symbolic remote refs are excluded from `remote_branch_refs`.
- New persisted config field must deserialize old config files that lack it (`#[serde(default)]`).

---

## Task 1: `git-core` — populate `remote_branch_refs` on `GraphCommit`

**Files:**
- Modify: `crates/git-core/src/graph.rs:1-88`
- Test: `crates/git-core/tests/graph.rs`

**Interfaces:**
- Produces: `GraphCommit.remote_branch_refs: Vec<String>` (public field, same visibility as existing `branch_refs`).

- [ ] **Step 1: Write the failing tests**

Append to `crates/git-core/tests/graph.rs`:

```rust
#[test]
fn graph_log_reports_remote_branch_refs_for_a_tip_commit() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let oid = repo.head().unwrap().target().unwrap();
    repo.reference("refs/remotes/origin/main", oid, true, "test")
        .unwrap();

    let commits = git_core::graph::graph_log(&repo, 10, None).unwrap();

    assert_eq!(
        commits[0].remote_branch_refs,
        vec!["origin/main".to_string()]
    );
}

#[test]
fn graph_log_excludes_a_remote_symbolic_head_ref() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let oid = repo.head().unwrap().target().unwrap();
    repo.reference("refs/remotes/origin/main", oid, true, "test")
        .unwrap();
    repo.reference_symbolic("refs/remotes/origin/HEAD", "refs/remotes/origin/main", true, "test")
        .unwrap();

    let commits = git_core::graph::graph_log(&repo, 10, None).unwrap();

    // Only the direct ref should show up — the symbolic HEAD alias must not produce a
    // duplicate or an "origin/HEAD" badge.
    assert_eq!(
        commits[0].remote_branch_refs,
        vec!["origin/main".to_string()]
    );
}

#[test]
fn graph_log_keeps_local_and_remote_branch_refs_independent() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    let oid = repo.head().unwrap().target().unwrap();
    repo.reference("refs/remotes/origin/main", oid, true, "test")
        .unwrap();

    let commits = git_core::graph::graph_log(&repo, 10, None).unwrap();

    assert_eq!(commits[0].branch_refs, vec![main_branch]);
    assert_eq!(
        commits[0].remote_branch_refs,
        vec!["origin/main".to_string()]
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test graph -- graph_log_reports_remote_branch_refs_for_a_tip_commit`
Expected: FAIL — compile error, `GraphCommit` has no field `remote_branch_refs`.

- [ ] **Step 3: Implement**

In `crates/git-core/src/graph.rs`:

Change the import line (currently `use git2::{BranchType, Oid, Repository, Sort};`) to:

```rust
use git2::{BranchType, Oid, ReferenceType, Repository, Sort};
```

Add the field to the struct (after `pub branch_refs: Vec<String>,`):

```rust
    pub remote_branch_refs: Vec<String>,
```

Inside `graph_log`, after the existing local `tips_by_oid` build loop (right before `let mut revwalk = repo.revwalk()?;`), add:

```rust
    let mut remote_tips_by_oid: HashMap<Oid, Vec<String>> = HashMap::new();
    for entry in repo.branches(Some(BranchType::Remote))? {
        let (branch, _) = entry?;
        // Symbolic refs like `origin/HEAD` point at another ref rather than a commit
        // directly and would otherwise duplicate whatever branch they alias — skip them so
        // they don't produce a fake "origin/HEAD" badge on every default-branch commit.
        if branch.get().kind() != Some(ReferenceType::Direct) {
            continue;
        }
        let Ok(Some(name)) = branch.name() else {
            continue;
        };
        if let Some(oid) = branch.get().target() {
            remote_tips_by_oid.entry(oid).or_default().push(name.to_string());
        }
    }
```

Inside the commit-building loop, after `let branch_refs = tips_by_oid.get(&oid).cloned().unwrap_or_default();`, add:

```rust
        let remote_branch_refs = remote_tips_by_oid.get(&oid).cloned().unwrap_or_default();
```

And add `remote_branch_refs,` to the `GraphCommit { ... }` struct literal, after `branch_refs,`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p git-core --test graph`
Expected: PASS (all tests in the file, including the 3 new ones and the pre-existing ones — the struct-literal change must not break any existing construction site).

- [ ] **Step 5: Commit**

```bash
git add crates/git-core/src/graph.rs crates/git-core/tests/graph.rs
git commit -m "feat(git-core): populate remote branch tips on GraphCommit"
```

---

## Task 2: `config` — persisted remote branch graph-visibility selection

**Files:**
- Modify: `crates/config/src/lib.rs:1-330`
- Test: Create `crates/config/tests/graph_remote_branch_selection.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces: `config::get_graph_remote_branch_selection(repo_path: &Path) -> Result<Option<Vec<String>>, ConfigError>`, `config::set_graph_remote_branch_selection(repo_path: &Path, selected: &[String]) -> Result<(), ConfigError>`, and the `_at(config_file, ...)` variants of both (same shape as the existing local pair at `crates/config/src/lib.rs:285-322`).

- [ ] **Step 1: Write the failing tests**

Create `crates/config/tests/graph_remote_branch_selection.rs` (mirrors `crates/config/tests/graph_branch_selection.rs` exactly, function names/prefix swapped):

```rust
use std::path::{Path, PathBuf};

use config::{get_graph_remote_branch_selection_at, set_graph_remote_branch_selection_at};

#[test]
fn get_graph_remote_branch_selection_at_returns_none_for_a_repo_with_no_saved_selection() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    let result = get_graph_remote_branch_selection_at(&config_file, Path::new("/repo/a")).unwrap();

    assert_eq!(result, None);
}

#[test]
fn set_graph_remote_branch_selection_at_persists_and_round_trips() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    let repo_path = PathBuf::from("/repo/a");

    set_graph_remote_branch_selection_at(&config_file, &repo_path, &["origin/main".to_string()])
        .unwrap();

    let result = get_graph_remote_branch_selection_at(&config_file, &repo_path).unwrap();
    assert_eq!(result, Some(vec!["origin/main".to_string()]));
}

#[test]
fn set_graph_remote_branch_selection_at_overwrites_a_previous_selection_for_the_same_repo() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    let repo_path = PathBuf::from("/repo/a");

    set_graph_remote_branch_selection_at(&config_file, &repo_path, &["origin/main".to_string()])
        .unwrap();
    set_graph_remote_branch_selection_at(
        &config_file,
        &repo_path,
        &["origin/main".to_string(), "origin/feature".to_string()],
    )
    .unwrap();

    let result = get_graph_remote_branch_selection_at(&config_file, &repo_path).unwrap();
    assert_eq!(
        result,
        Some(vec!["origin/main".to_string(), "origin/feature".to_string()])
    );
}

#[test]
fn set_graph_remote_branch_selection_at_keeps_selections_for_other_repos_separate() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    set_graph_remote_branch_selection_at(&config_file, Path::new("/repo/a"), &["origin/main".to_string()])
        .unwrap();
    set_graph_remote_branch_selection_at(&config_file, Path::new("/repo/b"), &["origin/dev".to_string()])
        .unwrap();

    assert_eq!(
        get_graph_remote_branch_selection_at(&config_file, Path::new("/repo/a")).unwrap(),
        Some(vec!["origin/main".to_string()])
    );
    assert_eq!(
        get_graph_remote_branch_selection_at(&config_file, Path::new("/repo/b")).unwrap(),
        Some(vec!["origin/dev".to_string()])
    );
}

#[test]
fn get_graph_remote_branch_selection_at_reads_a_config_file_missing_the_key() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    // A config file with no `graph_remote_branch_selections` key at all — simulates an old
    // config file written before this feature existed.
    std::fs::write(&config_file, "recent_repos = []\n").unwrap();

    let result = get_graph_remote_branch_selection_at(&config_file, Path::new("/repo/a")).unwrap();

    assert_eq!(result, None);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p config --test graph_remote_branch_selection`
Expected: FAIL — compile error, no such functions in the `config` crate.

- [ ] **Step 3: Implement**

In `crates/config/src/lib.rs`, after the existing `GraphBranchSelection` struct (`lib.rs:36-40`), add:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphRemoteBranchSelection {
    pub repo_path: PathBuf,
    pub selected_branches: Vec<String>,
}
```

In the `ConfigFile` struct (`lib.rs:107-124`), after `graph_branch_selections: Vec<GraphBranchSelection>,`, add:

```rust
    #[serde(default)]
    graph_remote_branch_selections: Vec<GraphRemoteBranchSelection>,
```

After the existing `set_graph_branch_selection_at` function (`lib.rs:308-322`), add the four new functions, mirroring the local ones exactly:

```rust
pub fn get_graph_remote_branch_selection(repo_path: &Path) -> Result<Option<Vec<String>>, ConfigError> {
    get_graph_remote_branch_selection_at(&config_file_path()?, repo_path)
}

pub fn set_graph_remote_branch_selection(
    repo_path: &Path,
    selected_branches: &[String],
) -> Result<(), ConfigError> {
    set_graph_remote_branch_selection_at(&config_file_path()?, repo_path, selected_branches)
}

pub fn get_graph_remote_branch_selection_at(
    config_file: &Path,
    repo_path: &Path,
) -> Result<Option<Vec<String>>, ConfigError> {
    let config = read_config(config_file)?;
    Ok(config
        .graph_remote_branch_selections
        .into_iter()
        .find(|s| s.repo_path == repo_path)
        .map(|s| s.selected_branches))
}

pub fn set_graph_remote_branch_selection_at(
    config_file: &Path,
    repo_path: &Path,
    selected_branches: &[String],
) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    config
        .graph_remote_branch_selections
        .retain(|s| s.repo_path != repo_path);
    config.graph_remote_branch_selections.push(GraphRemoteBranchSelection {
        repo_path: repo_path.to_path_buf(),
        selected_branches: selected_branches.to_vec(),
    });
    write_config(config_file, &config)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p config`
Expected: PASS (new test file plus every pre-existing `config` test, since `ConfigFile` gained a field).

- [ ] **Step 5: Commit**

```bash
git add crates/config/src/lib.rs crates/config/tests/graph_remote_branch_selection.rs
git commit -m "feat(config): persist per-repo remote branch graph-visibility selection"
```

---

## Task 3: `tauri-app` — commands and DTO field

**Files:**
- Modify: `crates/tauri-app/src/commands/status.rs:35-47`
- Modify: `crates/tauri-app/src/commands/mod.rs:38-42,432-443`
- Modify: `crates/tauri-app/src/main.rs:5-23,51-58`

**Interfaces:**
- Consumes: `config::get_graph_remote_branch_selection`, `config::set_graph_remote_branch_selection` (Task 2), `GraphCommit.remote_branch_refs` (Task 1).
- Produces: Tauri commands `get_graph_remote_branch_selection`, `set_graph_remote_branch_selection`; `GraphCommitDto.remote_branch_refs: Vec<String>` (serialized as `remoteBranchRefs`).

- [ ] **Step 1: Add the two commands**

In `crates/tauri-app/src/commands/status.rs`, after the existing `set_graph_branch_selection` command (ends at line 47), add:

```rust
#[tauri::command]
pub fn get_graph_remote_branch_selection(repo_path: String) -> Result<Option<Vec<String>>, String> {
    config::get_graph_remote_branch_selection(Path::new(&repo_path)).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_graph_remote_branch_selection(
    repo_path: String,
    selected_branches: Vec<String>,
) -> Result<(), String> {
    config::set_graph_remote_branch_selection(Path::new(&repo_path), &selected_branches)
        .map_err(|error| error.to_string())
}
```

- [ ] **Step 2: Add the DTO field**

In `crates/tauri-app/src/commands/mod.rs`, in the `GraphCommitDto` struct (`mod.rs:432-443`), after `pub branch_refs: Vec<String>,`, add:

```rust
    pub remote_branch_refs: Vec<String>,
```

In its `From<git_core::graph::GraphCommit>` impl right below, after `branch_refs: c.branch_refs,`, add:

```rust
            remote_branch_refs: c.remote_branch_refs,
```

- [ ] **Step 3: Export the new commands**

In `crates/tauri-app/src/commands/mod.rs:38-42`, change:

```rust
pub use status::{
    commit, discard_hunk, get_blame, get_commit_diff, get_commit_files, get_commit_graph,
    get_commit_message, get_graph_branch_selection, get_status, get_working_diff,
    set_graph_branch_selection, stage_file, stage_hunk, unstage_file, unstage_hunk,
};
```

to:

```rust
pub use status::{
    commit, discard_hunk, get_blame, get_commit_diff, get_commit_files, get_commit_graph,
    get_commit_message, get_graph_branch_selection, get_graph_remote_branch_selection,
    get_status, get_working_diff, set_graph_branch_selection,
    set_graph_remote_branch_selection, stage_file, stage_hunk, unstage_file, unstage_hunk,
};
```

- [ ] **Step 4: Register in `main.rs`**

In `crates/tauri-app/src/main.rs`, add `get_graph_remote_branch_selection` and `set_graph_remote_branch_selection` to the `use commands::{...}` import block (`main.rs:5-23`, alphabetically among the other `get_graph_branch_selection`/`set_graph_branch_selection` entries), and add both names to the `tauri::generate_handler![...]` list (`main.rs:51-58`), directly after the existing `get_graph_branch_selection, set_graph_branch_selection,` lines:

```rust
            get_graph_branch_selection,
            set_graph_branch_selection,
            get_graph_remote_branch_selection,
            set_graph_remote_branch_selection,
```

- [ ] **Step 5: Verify it builds**

Run: `cargo build -p tauri-app`
Expected: builds with no errors. (Per this repo's testing conventions, `tauri-app` pass-through commands rely on `git-core`/`config` coverage rather than their own tests — Task 1 and Task 2 already cover the logic this wraps.)

- [ ] **Step 6: Commit**

```bash
git add crates/tauri-app/src/commands/status.rs crates/tauri-app/src/commands/mod.rs crates/tauri-app/src/main.rs
git commit -m "feat(tauri-app): expose remote branch refs and graph-visibility commands"
```

---

## Task 4: `vscode-sidecar` — commands and DTO field

**Files:**
- Modify: `crates/vscode-sidecar/src/dispatch.rs:63-145,235-272,500-523`
- Test: `crates/vscode-sidecar/tests/protocol_roundtrip.rs`

**Interfaces:**
- Consumes: same as Task 3, plus `RepoPathParams` (already defined at `dispatch.rs:185-187`, reused as-is).
- Produces: JSON-RPC methods `"get_graph_remote_branch_selection"`, `"set_graph_remote_branch_selection"`; sidecar-local `GraphCommitDto.remote_branch_refs` (camelCase `remoteBranchRefs`).

- [ ] **Step 1: Write the failing tests**

In `crates/vscode-sidecar/tests/protocol_roundtrip.rs`, extend the existing `commit_graph_reflects_a_commit_through_the_sidecar` test (around line 207) by adding, right after `assert!(commits[0]["parentIds"].is_array());`:

```rust
    assert!(commits[0]["remoteBranchRefs"].is_array());
```

Then add a new test after `graph_branch_selection_round_trips` (ends at line 450):

```rust
#[test]
fn graph_remote_branch_selection_round_trips() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let (dir, _repo) = init_repo();
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);

    let before = sidecar.call(
        1,
        "get_graph_remote_branch_selection",
        serde_json::json!({"repoPath": repo_path}),
    );
    assert_eq!(before["result"], serde_json::Value::Null);

    let set = sidecar.call(
        2,
        "set_graph_remote_branch_selection",
        serde_json::json!({"repoPath": repo_path, "selectedBranches": ["origin/main"]}),
    );
    assert_eq!(set["result"], serde_json::Value::Null);

    let after = sidecar.call(
        3,
        "get_graph_remote_branch_selection",
        serde_json::json!({"repoPath": repo_path}),
    );
    assert_eq!(after["result"], serde_json::json!(["origin/main"]));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vscode-sidecar --test protocol_roundtrip -- graph_remote_branch_selection_round_trips`
Expected: FAIL — `"get_graph_remote_branch_selection"` is not a recognized method (dispatch returns an unknown-method error).

- [ ] **Step 3: Implement**

In `crates/vscode-sidecar/src/dispatch.rs`, add the two new match arms in `dispatch()` (`dispatch.rs:63-145`), directly after the existing `"set_graph_branch_selection" => set_graph_branch_selection(params),` line:

```rust
        "get_graph_remote_branch_selection" => get_graph_remote_branch_selection(params),
        "set_graph_remote_branch_selection" => set_graph_remote_branch_selection(params),
```

In the `GraphCommitDto` struct (`dispatch.rs:235-246`), after `branch_refs: Vec<String>,`, add:

```rust
    remote_branch_refs: Vec<String>,
```

In its `From<GraphCommit>` impl right below, after `branch_refs: c.branch_refs,`, add:

```rust
            remote_branch_refs: c.remote_branch_refs,
```

After the existing `set_graph_branch_selection` function (`dispatch.rs:516-522`), add:

```rust
fn get_graph_remote_branch_selection(params: Value) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let selection = config::get_graph_remote_branch_selection(Path::new(&params.repo_path))
        .map_err(|error| error.to_string())?;
    serde_json::to_value(selection).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetGraphRemoteBranchSelectionParams {
    repo_path: String,
    selected_branches: Vec<String>,
}

fn set_graph_remote_branch_selection(params: Value) -> Result<Value, String> {
    let params: SetGraphRemoteBranchSelectionParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    config::set_graph_remote_branch_selection(Path::new(&params.repo_path), &params.selected_branches)
        .map_err(|error| error.to_string())?;
    Ok(Value::Null)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vscode-sidecar --test protocol_roundtrip`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vscode-sidecar/src/dispatch.rs crates/vscode-sidecar/tests/protocol_roundtrip.rs
git commit -m "feat(vscode-sidecar): expose remote branch refs and graph-visibility methods"
```

---

## Task 5: Frontend IPC contract — `RepoClient`, both implementations, existing `GraphCommit` fixtures

**Files:**
- Modify: `frontend/src/ipc/RepoClient.ts:106-115,238-239`
- Modify: `frontend/src/ipc/tauriRepoClient.ts:83-86`
- Modify: `frontend/src/ipc/vscodeRepoClient.ts:207-210`
- Modify: `frontend/src/ipc/vscodeRepoClient.test.ts` (add wiring test)
- Modify: `frontend/src/state/useAppState.test.ts`, `frontend/src/components/CommitGraph.test.tsx`, `frontend/src/components/CommitHeader.test.tsx`, `frontend/src/lib/commitGraphLayout.test.ts` (fix now-incomplete `GraphCommit` fixtures)

**Interfaces:**
- Consumes: nothing new from earlier tasks (this is the TypeScript mirror of Tasks 3/4's wire contract).
- Produces: `GraphCommit.remoteBranchRefs: string[]`; `RepoClient.getGraphRemoteBranchSelection(repoPath: string): Promise<string[] | null>`; `RepoClient.setGraphRemoteBranchSelection(repoPath: string, selectedBranches: string[]): Promise<void>`.

- [ ] **Step 1: Write the failing test**

In `frontend/src/ipc/vscodeRepoClient.test.ts`, after the existing `it("wires getGraphBranchSelection and setGraphBranchSelection", ...)` test (ends at line 284), add:

```typescript
  it("wires getGraphRemoteBranchSelection and setGraphRemoteBranchSelection", async () => {
    const getPromise = vscodeRepoClient.getGraphRemoteBranchSelection("/repo");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "get_graph_remote_branch_selection",
      params: { repoPath: "/repo" },
    });
    respond(1, ["origin/main"]);
    await expect(getPromise).resolves.toEqual(["origin/main"]);

    const setPromise = vscodeRepoClient.setGraphRemoteBranchSelection("/repo", ["origin/main"]);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "set_graph_remote_branch_selection",
      params: { repoPath: "/repo", selectedBranches: ["origin/main"] },
    });
    respond(2, null);
    await expect(setPromise).resolves.toBeNull();
  });
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd frontend && pnpm test -- --run vscodeRepoClient.test.ts`
Expected: FAIL — `vscodeRepoClient.getGraphRemoteBranchSelection` is not a function.

- [ ] **Step 3: Implement**

In `frontend/src/ipc/RepoClient.ts`, add `remoteBranchRefs: string[];` to the `GraphCommit` interface (`RepoClient.ts:106-115`), after `branchRefs: string[];`. Add the two new methods to the `RepoClient` interface, after `setGraphBranchSelection` (`RepoClient.ts:238-239`):

```typescript
  getGraphRemoteBranchSelection(repoPath: string): Promise<string[] | null>;
  setGraphRemoteBranchSelection(repoPath: string, selectedBranches: string[]): Promise<void>;
```

In `frontend/src/ipc/tauriRepoClient.ts`, after `setGraphBranchSelection` (`tauriRepoClient.ts:85-86`):

```typescript
  getGraphRemoteBranchSelection: (repoPath: string) =>
    loggedInvoke<string[] | null>("get_graph_remote_branch_selection", { repoPath }),
  setGraphRemoteBranchSelection: (repoPath: string, selectedBranches: string[]) =>
    loggedInvoke("set_graph_remote_branch_selection", { repoPath, selectedBranches }),
```

In `frontend/src/ipc/vscodeRepoClient.ts`, after `setGraphBranchSelection` (`vscodeRepoClient.ts:209-210`):

```typescript
  getGraphRemoteBranchSelection: (repoPath: string) =>
    call<string[] | null>("get_graph_remote_branch_selection", { repoPath }),
  setGraphRemoteBranchSelection: (repoPath: string, selectedBranches: string[]) =>
    call<void>("set_graph_remote_branch_selection", { repoPath, selectedBranches }),
```

- [ ] **Step 4: Fix existing `GraphCommit` fixtures**

`GraphCommit.remoteBranchRefs` is now required, so every existing test literal that builds a `GraphCommit` (or a `Partial<GraphCommit>` spread over one) fails to type-check. Add `remoteBranchRefs: [],` immediately after each existing `branchRefs: [...],` line in:

- `frontend/src/state/useAppState.test.ts` (2 occurrences)
- `frontend/src/components/CommitGraph.test.tsx` (6 occurrences — including the `{ ...commits[0], branchRefs: ["main"] }` spread at line 224, which becomes `{ ...commits[0], branchRefs: ["main"], remoteBranchRefs: [] }`)
- `frontend/src/components/CommitHeader.test.tsx` (1 occurrence)
- `frontend/src/lib/commitGraphLayout.test.ts` (1 occurrence)

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd frontend && pnpm test -- --run`
Expected: PASS (full frontend suite — confirms every fixture was fixed and nothing else broke).

- [ ] **Step 6: Commit**

```bash
git add frontend/src/ipc/RepoClient.ts frontend/src/ipc/tauriRepoClient.ts frontend/src/ipc/vscodeRepoClient.ts frontend/src/ipc/vscodeRepoClient.test.ts frontend/src/state/useAppState.test.ts frontend/src/components/CommitGraph.test.tsx frontend/src/components/CommitHeader.test.tsx frontend/src/lib/commitGraphLayout.test.ts
git commit -m "feat(frontend): add remoteBranchRefs and remote graph-selection to RepoClient"
```

---

## Task 6: `useAppState` — load, persist, and expose remote graph-visibility selection

**Files:**
- Modify: `frontend/src/state/useAppState.ts:40-94,195-286,330-346`
- Modify: `frontend/src/state/useBranchActions.ts`
- Modify: `frontend/src/state/useAppState.test.ts` (new test)
- Modify: `frontend/src/App.tsx:318-319`

**Interfaces:**
- Consumes: `client.getGraphRemoteBranchSelection`, `client.setGraphRemoteBranchSelection` (Task 5).
- Produces: `AppState.graphRemoteBranchSelection: string[] | null`; `UseAppStateResult.setGraphRemoteBranchSelection(selectedBranches: string[]): Promise<void>`.

- [ ] **Step 1: Write the failing test**

In `frontend/src/state/useAppState.test.ts`, find the existing test that exercises `setGraphBranchSelection` (search for `"setGraphBranchSelection"` in the file) and add an analogous test directly after it:

```typescript
  it("persists graph remote branch selection and reloads it on refresh", async () => {
    client.getGraphRemoteBranchSelection = vi.fn().mockResolvedValue(["origin/main"]);
    client.setGraphRemoteBranchSelection = vi.fn().mockResolvedValue(undefined);
    const { result } = renderHook(() => useAppState(client, repoPath));
    await waitFor(() => expect(result.current.state.graphRemoteBranchSelection).toEqual(["origin/main"]));

    await act(async () => {
      await result.current.setGraphRemoteBranchSelection(["origin/main", "origin/feature"]);
    });

    expect(client.setGraphRemoteBranchSelection).toHaveBeenCalledWith(repoPath, [
      "origin/main",
      "origin/feature",
    ]);
  });
```

(Match this test's exact mock/render setup — `client`, `repoPath`, `renderHook`, `waitFor`, `act` — to whatever the neighboring `setGraphBranchSelection` test in the same file already uses; this file's existing tests are the source of truth for the harness shape.)

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd frontend && pnpm test -- --run useAppState.test.ts`
Expected: FAIL — `result.current.setGraphRemoteBranchSelection` is not a function.

- [ ] **Step 3: Implement**

In `frontend/src/state/useAppState.ts`, add to the `AppState` interface (`useAppState.ts:40-94`), after `graphBranchSelection: string[] | null;`:

```typescript
  // Independent of graphBranchSelection: this one never affects which commits `graph_log`
  // walks, only which remote-branch badges CommitGraph renders. `null` means "show all".
  graphRemoteBranchSelection: string[] | null;
```

Add to the `UseAppStateResult` interface, next to the existing `setGraphBranchSelection(selectedBranches: string[]): Promise<void>;` line:

```typescript
  setGraphRemoteBranchSelection(selectedBranches: string[]): Promise<void>;
```

In the initial state object (`useAppState.ts:195-225`), after `graphBranchSelection: null,`:

```typescript
    graphRemoteBranchSelection: null,
```

In `refresh()` (`useAppState.ts:231-286`), change:

```typescript
      const graphBranchSelection = await client.getGraphBranchSelection(repoPath);
```

to:

```typescript
      const [graphBranchSelection, graphRemoteBranchSelection] = await Promise.all([
        client.getGraphBranchSelection(repoPath),
        client.getGraphRemoteBranchSelection(repoPath),
      ]);
```

and add `graphRemoteBranchSelection,` to the `setState((prev) => ({ ... }))` call, next to the existing `graphBranchSelection,` line.

In `frontend/src/state/useBranchActions.ts`, add to the `BranchActions` interface, after `setGraphBranchSelection(selectedBranches: string[]): Promise<void>;`:

```typescript
  setGraphRemoteBranchSelection(selectedBranches: string[]): Promise<void>;
```

Inside `useBranchActions`, after the existing `setGraphBranchSelection` callback:

```typescript
  const setGraphRemoteBranchSelection = useCallback(
    (selectedBranches: string[]) =>
      runMutation(() => client.setGraphRemoteBranchSelection(repoPath, selectedBranches)),
    [client, runMutation, repoPath],
  );
```

Add `setGraphRemoteBranchSelection,` to the function's `return { ... }` object.

Back in `frontend/src/state/useAppState.ts`, destructure it from `useBranchActions(...)` (next to the existing `setGraphBranchSelection,` at line ~338) and add it to `useAppState`'s own returned object (next to the existing `setGraphBranchSelection,` at line ~486).

In `frontend/src/App.tsx`, add the two new props to the existing `<BranchTree ...>` call, next to `graphBranchSelection`/`onSetGraphBranchSelection` (`App.tsx:318-319`):

```typescript
              graphRemoteBranchSelection={appState.state.graphRemoteBranchSelection}
              onSetGraphRemoteBranchSelection={appState.setGraphRemoteBranchSelection}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && pnpm test -- --run`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/state/useAppState.ts frontend/src/state/useBranchActions.ts frontend/src/state/useAppState.test.ts frontend/src/App.tsx
git commit -m "feat(frontend): load and persist graph remote branch selection in app state"
```

---

## Task 7: `BranchTree` — toggle for remote branch graph visibility

**Files:**
- Modify: `frontend/src/components/BranchTree.tsx:38-...(props),300-364,525-529`
- Modify: `frontend/src/components/BranchTree.test.tsx` (new test)

**Interfaces:**
- Consumes: `graphRemoteBranchSelection: string[] | null` and `onSetGraphRemoteBranchSelection: (selected: string[]) => void` (Task 6, threaded from `App.tsx`).
- Produces: no new exports — this only changes `BranchTree`'s rendered DOM (an extra swatch/toggle button per remote-branch row).

- [ ] **Step 1: Write the failing test**

In `frontend/src/components/BranchTree.test.tsx`, find an existing test that opens a remote folder and asserts on a remote branch row (search for `renderRemoteNodes` usage context, e.g. a test that sets up `onListRemoteBranches` and expands a remote). Add a new test alongside it:

```typescript
  it("toggles a remote branch's graph visibility via its swatch button", async () => {
    const onSetGraphRemoteBranchSelection = vi.fn();
    render(
      <BranchTree
        {...defaultProps}
        remotes={[{ name: "origin", fetchUrl: "https://example.com/repo.git", pushUrl: null }]}
        graphRemoteBranchSelection={null}
        onSetGraphRemoteBranchSelection={onSetGraphRemoteBranchSelection}
        onListRemoteBranches={vi.fn().mockResolvedValue(["main"])}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    await screen.findByText("main");

    fireEvent.click(screen.getByRole("button", { name: "Show origin/main in graph" }));

    expect(onSetGraphRemoteBranchSelection).toHaveBeenCalledWith(["origin/main"]);
  });
```

(Match `defaultProps`, the remote-folder open interaction, and the exact `RemoteInfo` shape to whatever this test file's existing remote-branch tests already use — they're the source of truth for the harness shape and prop defaults in this file.)

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd frontend && pnpm test -- --run BranchTree.test.tsx`
Expected: FAIL — no button named "Show origin/main in graph" exists yet (and/or a TypeScript error for the two new required props, depending on how `defaultProps` is typed in this file).

- [ ] **Step 3: Implement**

In `BranchTree`'s prop destructuring and type (`BranchTree.tsx:38-...`), add `graphRemoteBranchSelection` and `onSetGraphRemoteBranchSelection` next to the existing `graphBranchSelection`/`onSetGraphBranchSelection` props:

```typescript
  graphRemoteBranchSelection: string[] | null;
  onSetGraphRemoteBranchSelection: (selectedBranches: string[]) => void;
```

After the existing `toggleGraphBranch` function (`BranchTree.tsx:525-529`), add:

```typescript
  const allKnownRemoteBranches = Object.entries(remoteBranches).flatMap(([remoteName, names]) =>
    (names ?? []).map((name) => `${remoteName}/${name}`),
  );
  const toggleGraphRemoteBranch = (qualifiedName: string) => {
    const shown = graphRemoteBranchSelection ?? allKnownRemoteBranches;
    const next = shown.includes(qualifiedName)
      ? shown.filter((n) => n !== qualifiedName)
      : [...shown, qualifiedName];
    onSetGraphRemoteBranchSelection(next);
  };
```

In `renderRemoteNodes` (`BranchTree.tsx:300-364`), inside the branch-row `<div className={styles.rowInner}>` (currently starting with `<span className={styles.name} ...>`), add a swatch button before that span, mirroring the local row's swatch (`BranchTree.tsx:245-255`):

```typescript
            <button
              type="button"
              className={styles.swatch}
              aria-label={`Show ${remoteName}/${branchName} in graph`}
              aria-pressed={(graphRemoteBranchSelection ?? allKnownRemoteBranches).includes(
                `${remoteName}/${branchName}`,
              )}
              style={{ "--swatch": branchSwatchColor(`${remoteName}/${branchName}`) } as CSSProperties}
              onClick={(event) => {
                event.stopPropagation();
                toggleGraphRemoteBranch(`${remoteName}/${branchName}`);
              }}
            />
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && pnpm test -- --run BranchTree.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/BranchTree.tsx frontend/src/components/BranchTree.test.tsx
git commit -m "feat(frontend): add graph-visibility toggle to remote branch rows"
```

---

## Task 8: `CommitGraph` — render remote branch badges

**Files:**
- Modify: `frontend/src/components/CommitGraph.tsx:20-...(props),214-218`
- Modify: `frontend/src/components/CommitGraph.module.css:30-38`
- Modify: `frontend/src/components/CommitGraph.test.tsx`
- Modify: `frontend/src/App.tsx:436-447`

**Interfaces:**
- Consumes: `commit.remoteBranchRefs: string[]` (Task 5), `graphRemoteBranchSelection` (Task 6, passed down from `App.tsx`).
- Produces: no new exports — this only changes `CommitGraph`'s rendered DOM.

- [ ] **Step 1: Write the failing test**

In `frontend/src/components/CommitGraph.test.tsx`, after the existing `it("renders a branch badge for a commit that is a branch tip", ...)` test, add:

```typescript
  it("renders a remote branch badge for a commit that is a remote-tracking tip", () => {
    const commitsWithRemoteBranch: GraphCommit[] = [
      { ...commits[0], remoteBranchRefs: ["origin/main"] },
      commits[1],
    ];
    render(
      <CommitGraph
        status={status}
        commits={commitsWithRemoteBranch}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    expect(screen.getByText("origin/main")).toBeInTheDocument();
  });

  it("hides a remote branch badge not in graphRemoteBranchSelection", () => {
    const commitsWithRemoteBranch: GraphCommit[] = [
      { ...commits[0], remoteBranchRefs: ["origin/main"] },
      commits[1],
    ];
    render(
      <CommitGraph
        status={status}
        commits={commitsWithRemoteBranch}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
        graphRemoteBranchSelection={["origin/other"]}
      />,
    );

    expect(screen.queryByText("origin/main")).not.toBeInTheDocument();
  });
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm test -- --run CommitGraph.test.tsx`
Expected: FAIL — `"origin/main"` is never rendered, and `graphRemoteBranchSelection` is not an accepted prop.

- [ ] **Step 3: Implement**

In `frontend/src/components/CommitGraph.tsx`, add an optional prop to the destructured params and type (`CommitGraph.tsx:20-...`), next to `hasMore`/`onLoadMore`:

```typescript
  graphRemoteBranchSelection = null,
```

```typescript
  // `null`/omitted means "show every remote badge present" — mirrors BranchTree's local
  // `graphBranchSelection ?? branches.map(...)` fallback, but computed here from the commits
  // actually on screen since remote selection never narrows which commits are fetched.
  graphRemoteBranchSelection?: string[] | null;
```

Immediately before the `commits.map((commit, index) => ( ... ))` block, compute the fallback shown-set:

```typescript
  const shownRemoteBranches =
    graphRemoteBranchSelection ?? Array.from(new Set(commits.flatMap((c) => c.remoteBranchRefs)));
```

After the existing local badge loop (`CommitGraph.tsx:214-218`):

```typescript
          {commit.branchRefs.map((ref) => (
            <span key={ref} className={styles.branchBadge}>
              {ref}
            </span>
          ))}
```

add:

```typescript
          {commit.remoteBranchRefs
            .filter((ref) => shownRemoteBranches.includes(ref))
            .map((ref) => (
              <span key={ref} className={styles.remoteBranchBadge}>
                {ref}
              </span>
            ))}
```

In `frontend/src/components/CommitGraph.module.css`, after the existing `.branchBadge` rule (`CommitGraph.module.css:30-38`), add:

```css
.remoteBranchBadge {
  font-size: var(--text-xs);
  padding: 1px var(--space-2);
  border-radius: var(--radius-pill);
  background: var(--color-bg-subtle);
  color: var(--color-text-muted);
  border: 1px dashed var(--color-border);
  flex-shrink: 0;
}
```

In `frontend/src/App.tsx`, add the prop to the existing `<CommitGraph ...>` call (`App.tsx:436-447`), next to `onSquashCommits`:

```typescript
                graphRemoteBranchSelection={appState.state.graphRemoteBranchSelection}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && pnpm test -- --run`
Expected: PASS (full suite).

- [ ] **Step 5: Run the full check before wrapping up**

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cd frontend && pnpm lint && pnpm build
```

Expected: all green.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/CommitGraph.tsx frontend/src/components/CommitGraph.module.css frontend/src/components/CommitGraph.test.tsx frontend/src/App.tsx
git commit -m "feat(frontend): render remote branch badges in the commit graph"
```

---

## Post-implementation: CHANGELOG

Per this repo's pre-push hook, any push touching `crates/`, `frontend/src/`, or `e2e/` needs a matching `CHANGELOG.md` entry. Add one describing the user-visible change (remote branch tips now show inline in the commit graph, with their own show/hide toggle in the branch tree) before pushing.
