# Multi-Branch Commit Graph Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `HistoryList`'s flat, current-branch-only commit list with a multi-branch
swimlane graph, per `docs/superpowers/specs/2026-08-13-commit-graph-design.md`.

**Architecture:** A new `git-core::graph` module (`graph_log`) supersedes `git-core::log`
end-to-end — the replacement happens at every layer (`Command::GetCommitGraph` replaces
`Command::GetLog`, `getCommitGraph` replaces `getLog`, `commits: GraphCommit[]` replaces
`log: CommitInfo[]`), not an addition alongside it. `git-core::log` is deleted once nothing
references it (last task, since Rust never errors on an unused `pub` item within a lib crate —
only its *callers* need to move off it first, in dependency order). A new pure utility,
`commitGraphLayout.ts`, computes lane assignment with no DOM/React involved, independently
unit-tested. `HistoryList.tsx`/`.test.tsx` are renamed to `CommitGraph.tsx`/`.test.tsx` — same
component, new name, because it now does something categorically different.

**Tech Stack:** Rust (git2, thiserror), Tauri 2, React/TypeScript, Vitest + Testing Library,
`tauri-driver` + WebdriverIO for E2E.

## Global Constraints

- Local branches only, all of them shown together (not just current branch) — the point of
  "multi-branch".
- Full swimlane rendering (colored lines, merge connections, branch-tip labels) — not a
  simplified flat-list-with-badges fallback.
- Lane assignment is frontend logic (`commitGraphLayout.ts`), not backend — `git-core::graph`
  returns pure graph structure (commits, parent ids, branch refs), no lane/column indices.
- **Hard backward-compatibility constraint, binding on every task that touches commit-row
  markup:** all four pre-existing E2E specs (`e2e/specs/first-flow.spec.ts`,
  `branch-management.spec.ts`, `stash-management.spec.ts`, `blame-viewer.spec.ts`) locate commit
  rows via a plain substring match against an `<li>`'s text content (e.g. `$("li*=e2e: first
  commit")`), and `blame-viewer.spec.ts` also asserts `aria-selected` directly on that `<li>`.
  Every commit row's outermost element must stay an `<li>` whose text content still includes
  `{shortId} {summary}` as a plain substring, with `aria-selected` still set on that same `<li>`.
  Any task that changes commit-row markup must run the full existing E2E suite, not just new
  tests, before being considered done.
- `git-core::log`/`CommitInfo`/`getLog` are **removed**, not kept alongside the new
  `graph`/`GraphCommit`/`getCommitGraph` — dead code once superseded.
- `git-core` tests use real temp-dir repos (`crates/git-core/tests/common/mod.rs` helpers), never
  mocks. `tauri-app` tests use real temp-dir repos too, inline in the module they test.
  `frontend` tests mock `RepoClient`, never `@tauri-apps/api`.
- Test commands: `cargo test -p git-core --test graph`, `cargo test -p tauri-app`,
  `cd frontend && pnpm test -- --run`, `cd e2e && pnpm test` (needs a fresh `pnpm build` +
  `cargo build --workspace --features tauri-app/custom-protocol` first, per `CLAUDE.md`).

---

### Task 1: `git-core::graph` — `graph_log`

**Files:**
- Create: `crates/git-core/src/graph.rs`
- Modify: `crates/git-core/src/lib.rs` (add `pub mod graph;` — `git-core::log` stays for now,
  removed only in Task 10 once nothing references it)
- Test: `crates/git-core/tests/graph.rs`

**Interfaces:**
- Consumes: `crates/git-core/tests/common/mod.rs`'s `init_repo()`, `commit_all()`, `write_file()`
  (already exist, unchanged); `git_core::branch::{list_branches, create_branch, switch_branch}`
  (already exist, unchanged) for test setup.
- Produces (used by Task 2):
  ```rust
  pub struct GraphCommit {
      pub id: String,
      pub short_id: String,
      pub summary: String,
      pub author_name: String,
      pub author_email: String,
      pub timestamp: i64,
      pub parent_ids: Vec<String>,
      pub branch_refs: Vec<String>,
  }
  pub enum GraphError { Read(git2::Error) }
  pub fn graph_log(repo: &git2::Repository, limit: usize) -> Result<Vec<GraphCommit>, GraphError>;
  ```

- [ ] **Step 1: Write the failing tests**

Create `crates/git-core/tests/graph.rs`:

```rust
mod common;

use common::{commit_all, init_repo, write_file};

#[test]
fn graph_log_returns_an_empty_vec_for_a_repository_with_no_commits() {
    let (_dir, repo) = init_repo();

    let result = git_core::graph::graph_log(&repo, 10).unwrap();

    assert!(result.is_empty());
}

#[test]
fn graph_log_shows_commits_from_every_local_branch() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");

    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "file.txt", "v2");
    commit_all(&repo, "feature commit");

    let commits = git_core::graph::graph_log(&repo, 10).unwrap();

    assert_eq!(commits.len(), 2);
    assert!(commits.iter().any(|c| c.summary == "feature commit"));
    assert!(commits.iter().any(|c| c.summary == "initial commit"));
}

#[test]
fn graph_log_reports_branch_refs_only_for_tip_commits() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let initial_branch = git_core::branch::list_branches(&repo).unwrap()[0].name.clone();

    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "file.txt", "v2");
    commit_all(&repo, "feature commit");
    git_core::branch::switch_branch(&repo, &initial_branch).unwrap();

    let commits = git_core::graph::graph_log(&repo, 10).unwrap();

    let feature_commit = commits.iter().find(|c| c.summary == "feature commit").unwrap();
    assert_eq!(feature_commit.branch_refs, vec!["feature".to_string()]);
    // "initial commit" is the initial branch's tip (feature has moved past it) — it should
    // carry the initial branch's name, not be empty.
    let initial_commit = commits.iter().find(|c| c.summary == "initial commit").unwrap();
    assert_eq!(initial_commit.branch_refs, vec![initial_branch]);
}

#[test]
fn graph_log_reports_multiple_parent_ids_for_a_merge_commit() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1");
    commit_all(&repo, "base commit");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0].name.clone();

    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "feature.txt", "v1");
    commit_all(&repo, "feature commit");
    let feature_commit_id = repo.head().unwrap().peel_to_commit().unwrap().id();

    git_core::branch::switch_branch(&repo, &main_branch).unwrap();
    write_file(dir.path(), "main.txt", "v1");
    commit_all(&repo, "main commit");
    let main_commit = repo.head().unwrap().peel_to_commit().unwrap();
    let feature_commit = repo.find_commit(feature_commit_id).unwrap();

    // No merge.rs exists yet (a future Phase 2 subsystem) — construct a two-parent commit
    // directly via the same low-level `repo.commit()` primitive `commit_all` already uses
    // elsewhere in this test suite, rather than needing real merge machinery just to test that
    // `graph_log` correctly reports every parent.
    let tree_id = repo.index().unwrap().write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = repo.signature().unwrap();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        "merge feature into main",
        &tree,
        &[&main_commit, &feature_commit],
    )
    .unwrap();

    let commits = git_core::graph::graph_log(&repo, 10).unwrap();

    let merge_commit = commits.iter().find(|c| c.summary == "merge feature into main").unwrap();
    assert_eq!(merge_commit.parent_ids.len(), 2);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test graph`
Expected: FAIL to compile — `git_core::graph` doesn't exist yet.

- [ ] **Step 3: Write the implementation**

Create `crates/git-core/src/graph.rs`:

```rust
use std::collections::HashMap;

use git2::{BranchType, Oid, Repository, Sort};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GraphError {
    #[error("failed to read commit graph: {0}")]
    Read(#[from] git2::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphCommit {
    pub id: String,
    pub short_id: String,
    pub summary: String,
    pub author_name: String,
    pub author_email: String,
    pub timestamp: i64,
    pub parent_ids: Vec<String>,
    pub branch_refs: Vec<String>,
}

pub fn graph_log(repo: &Repository, limit: usize) -> Result<Vec<GraphCommit>, GraphError> {
    let mut tips_by_oid: HashMap<Oid, Vec<String>> = HashMap::new();
    for entry in repo.branches(Some(BranchType::Local))? {
        let (branch, _) = entry?;
        let Ok(Some(name)) = branch.name() else {
            continue;
        };
        if let Some(oid) = branch.get().target() {
            tips_by_oid.entry(oid).or_default().push(name.to_string());
        }
    }

    let mut revwalk = repo.revwalk()?;
    revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
    // On a repo with no commits yet, there are no local branches to match, so this simply
    // pushes nothing — the loop below then runs zero times, giving an empty graph. Unlike
    // `push_head()` (which errors on an unborn HEAD, the case the removed `log()` had to
    // special-case), `push_glob` doesn't error on zero matches — no special-casing needed here.
    revwalk.push_glob("refs/heads/*")?;

    let mut commits = Vec::new();
    for oid_result in revwalk.take(limit) {
        let oid = oid_result?;
        let commit = repo.find_commit(oid)?;

        let id = oid.to_string();
        let short_id = id[..7].to_string();
        let summary = commit
            .summary()
            .ok()
            .flatten()
            .unwrap_or_default()
            .to_string();
        let author_name = commit.author().name().ok().unwrap_or_default().to_string();
        let author_email = commit.author().email().ok().unwrap_or_default().to_string();
        let timestamp = commit.time().seconds();
        let parent_ids = commit.parent_ids().map(|p| p.to_string()).collect();
        let branch_refs = tips_by_oid.get(&oid).cloned().unwrap_or_default();

        commits.push(GraphCommit {
            id,
            short_id,
            summary,
            author_name,
            author_email,
            timestamp,
            parent_ids,
            branch_refs,
        });
    }

    Ok(commits)
}
```

Add `pub mod graph;` to `crates/git-core/src/lib.rs` (the file is a flat alphabetical list —
`graph` sits between `diff` and `log`):

```rust
pub mod blame;
pub mod branch;
pub mod commit;
pub mod diff;
pub mod graph;
pub mod log;
pub mod repo;
pub mod stage;
pub mod stash;
pub mod status;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p git-core --test graph`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/git-core/src/graph.rs crates/git-core/src/lib.rs crates/git-core/tests/graph.rs
git commit -m "feat(git-core): add multi-branch commit graph"
```

---

### Task 2: `tauri-app::worker` — `GetCommitGraph` Command (replaces `GetLog`)

**Files:**
- Modify: `crates/tauri-app/src/worker.rs`

**Interfaces:**
- Consumes: `git_core::graph::{GraphCommit, graph_log}` from Task 1.
- Produces (used by Task 3):
  ```rust
  impl WorkerHandle {
      pub fn get_commit_graph(&self, limit: usize) -> Result<Vec<GraphCommit>, String>;
  }
  ```
  (`get_log` is removed, not kept.)

- [ ] **Step 1: Write the failing test**

In `crates/tauri-app/src/worker.rs`'s `#[cfg(test)] mod tests` block, replace the existing
`get_log_reflects_a_commit` test with:

```rust
    #[test]
    fn get_commit_graph_reflects_a_commit() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "file.txt", "hello");
        commit_all(&repo, "initial commit");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let commits = worker.handle().get_commit_graph(10).unwrap();

        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].summary, "initial commit");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p tauri-app`
Expected: FAIL to compile — `Command::GetCommitGraph`/`WorkerHandle::get_commit_graph` don't
exist yet, and the old `get_log_reflects_a_commit` test (now removed) no longer references
`get_log` either way, so this is purely additive-but-currently-uncompilable until Step 3.

- [ ] **Step 3: Write the implementation**

In `crates/tauri-app/src/worker.rs`:

Change the import line from
```rust
use git_core::log::CommitInfo;
```
to (alphabetical position moves — `graph` sits between `diff` and `stash`):
```rust
use git_core::graph::GraphCommit;
```

Replace the `Command::GetLog` variant with:
```rust
    GetCommitGraph {
        limit: usize,
        reply: Sender<Result<Vec<GraphCommit>, String>>,
    },
```
(same position in the enum — right after `GetStatus`, before `GetWorkingDiff`.)

Replace the `Command::GetLog` match arm with:
```rust
                    Command::GetCommitGraph { limit, reply } => {
                        let result =
                            git_core::graph::graph_log(&repo, limit).map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
```

Replace the `WorkerHandle::get_log` method with:
```rust
    pub fn get_commit_graph(&self, limit: usize) -> Result<Vec<GraphCommit>, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::GetCommitGraph {
                limit,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p tauri-app`
Expected: PASS (all existing tests, with `get_log_reflects_a_commit` now
`get_commit_graph_reflects_a_commit`).

- [ ] **Step 5: Commit**

```bash
git add crates/tauri-app/src/worker.rs
git commit -m "feat(tauri-app): replace GetLog with GetCommitGraph in the worker"
```

---

### Task 3: Tauri command — `get_commit_graph` (replaces `get_log`)

**Files:**
- Modify: `crates/tauri-app/src/commands.rs`
- Modify: `crates/tauri-app/src/main.rs`

**Interfaces:**
- Consumes: `WorkerHandle::get_commit_graph` from Task 2; `git_core::graph::GraphCommit` from
  Task 1.
- Produces (used by Task 4): a Tauri command `get_commit_graph`, returning
  `Result<Vec<GraphCommitDto>, String>`. `GraphCommitDto` serializes camelCase: `id`, `shortId`,
  `summary`, `authorName`, `authorEmail`, `timestamp`, `parentIds`, `branchRefs`. `get_log` is
  removed, not kept.

No dedicated test for this task: thin pass-through commands aren't separately tested per
`CLAUDE.md`'s convention, and `GraphCommitDto` uses plain field serialization, not enum `Debug`
formatting — no pinned wire-format test needed.

- [ ] **Step 1: Replace the DTO and command**

In `crates/tauri-app/src/commands.rs`, remove the `use git_core::log::CommitInfo;` import (the
crate no longer needs it — `commands.rs` only imports `git_core::diff::DiffHunk` directly
otherwise, everything else is referenced via its full `git_core::module::Type` path already).

Remove `CommitInfoDto` and its `impl From<CommitInfo>` block. Add in their place:

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphCommitDto {
    pub id: String,
    pub short_id: String,
    pub summary: String,
    pub author_name: String,
    pub author_email: String,
    pub timestamp: i64,
    pub parent_ids: Vec<String>,
    pub branch_refs: Vec<String>,
}

impl From<git_core::graph::GraphCommit> for GraphCommitDto {
    fn from(c: git_core::graph::GraphCommit) -> Self {
        GraphCommitDto {
            id: c.id,
            short_id: c.short_id,
            summary: c.summary,
            author_name: c.author_name,
            author_email: c.author_email,
            timestamp: c.timestamp,
            parent_ids: c.parent_ids,
            branch_refs: c.branch_refs,
        }
    }
}
```

Replace the `get_log` command with:

```rust
#[tauri::command]
pub async fn get_commit_graph(
    limit: usize,
    state: State<'_, AppState>,
) -> Result<Vec<GraphCommitDto>, String> {
    let commits = worker_handle(&state)?.get_commit_graph(limit)?;
    Ok(commits.into_iter().map(GraphCommitDto::from).collect())
}
```

- [ ] **Step 2: Update `main.rs`**

Replace the `use commands::{...}` import list with (fully alphabetized — `get_commit_graph`
sits between `get_commit_files` and `get_status`, `get_log` is gone):

```rust
use commands::{
    apply_stash, commit, create_branch, delete_branch, drop_stash, get_blame, get_commit_diff,
    get_commit_files, get_commit_graph, get_status, get_working_diff, list_branches,
    list_recent_repos, list_stashes, open_repo, pick_repo_folder, rename_branch, save_stash,
    stage_file, switch_branch, unstage_file, AppState,
};
```

In `tauri::generate_handler![...]`, replace `get_log,` in place with `get_commit_graph,` (same
position in the list, not moved — this is a like-for-like swap, not a new feature appended at
the end):

```rust
        .invoke_handler(tauri::generate_handler![
            open_repo,
            get_status,
            get_commit_graph,
            get_working_diff,
            get_commit_diff,
            get_commit_files,
            stage_file,
            unstage_file,
            commit,
            pick_repo_folder,
            list_recent_repos,
            list_branches,
            create_branch,
            switch_branch,
            delete_branch,
            rename_branch,
            list_stashes,
            save_stash,
            apply_stash,
            drop_stash,
            get_blame,
        ])
```

- [ ] **Step 3: Verify it builds**

Run: `cargo build --workspace`
Expected: builds cleanly.

- [ ] **Step 4: Run the full test suite**

Run: `cargo test --workspace`
Expected: PASS (all previously-passing tests still pass).

- [ ] **Step 5: Commit**

```bash
git add crates/tauri-app/src/commands.rs crates/tauri-app/src/main.rs
git commit -m "feat(tauri-app): replace get_log with get_commit_graph"
```

---

### Task 4: `RepoClient` + `useAppState` — replace `log`/`CommitInfo` with `commits`/`GraphCommit`

**Files:**
- Modify: `frontend/src/ipc/RepoClient.ts`
- Modify: `frontend/src/ipc/tauriRepoClient.ts`
- Modify: `frontend/src/state/useAppState.ts`
- Modify: `frontend/src/state/useAppState.test.ts`
- Modify: `frontend/src/components/DiffPane.test.tsx` (mechanical: rename one line in its
  `fakeClient` factory)
- Modify: `frontend/src/components/RepoPicker.test.tsx` (mechanical: rename one line in its
  `fakeClient` factory)

**Interfaces:**
- Consumes: Tauri command `get_commit_graph` from Task 3.
- Produces (used by Tasks 6–8):
  ```ts
  export interface GraphCommit {
    id: string;
    shortId: string;
    summary: string;
    authorName: string;
    authorEmail: string;
    timestamp: number;
    parentIds: string[];
    branchRefs: string[];
  }
  // on RepoClient:
  getCommitGraph(limit: number): Promise<GraphCommit[]>;
  // on AppState:
  commits: GraphCommit[];
  ```
- **Why this task is bigger than a typical single-layer task, and not split further:** removing
  `getLog` from the `RepoClient` interface breaks *every* implementer simultaneously — including
  `useAppState.ts` itself, which calls `client.getLog(...)` directly, not just its test file.
  Unlike adding a new method (where old code keeps compiling until something chooses to consume
  it — the pattern branch management, stash, and blame all used), there is no valid intermediate
  state where `RepoClient.ts` has removed `getLog` but `useAppState.ts` hasn't been updated yet —
  that's just broken, not a reviewable checkpoint. So this task does the interface change and
  every consumer of it (including its own state hook and every mock) together, keeping the build
  green from this task's first commit to its last.

- [ ] **Step 1: Replace the type and interface method**

In `frontend/src/ipc/RepoClient.ts`, remove `export interface CommitInfo { ... }` entirely and
add in its place (same position — after `StashEntry`, before `BlameLine`... actually `BlameLine`
was added after `StashEntry` by the blame feature, so the file's current interface order is
`StatusEntry, CommitInfo, DiffLine, DiffHunk, BranchInfo, StashEntry, BlameLine`. Replace
`CommitInfo` in place, keeping everything else's relative order unchanged):

```ts
export interface GraphCommit {
  id: string;
  shortId: string;
  summary: string;
  authorName: string;
  authorEmail: string;
  timestamp: number;
  parentIds: string[];
  branchRefs: string[];
}
```

In the `RepoClient` interface, replace `getLog(limit: number): Promise<CommitInfo[]>;` in place
with:

```ts
  getCommitGraph(limit: number): Promise<GraphCommit[]>;
```

- [ ] **Step 2: Update `tauriRepoClient.ts`**

Replace `CommitInfo` with `GraphCommit` in the type-only import (alphabetical position: `GraphCommit`
sits between `DiffHunk` and `RepoClient`):

```ts
import type {
  BlameLine,
  BranchInfo,
  DiffHunk,
  GraphCommit,
  RepoClient,
  StashEntry,
  StatusEntry,
} from "./RepoClient";
```

Replace `getLog: (limit: number) => invoke<CommitInfo[]>("get_log", { limit }),` in place with:

```ts
  getCommitGraph: (limit: number) =>
    invoke<GraphCommit[]>("get_commit_graph", { limit }),
```

- [ ] **Step 3: Update `useAppState.ts`**

Replace `CommitInfo` with `GraphCommit` in the type-only import:

```ts
import type {
  BranchInfo,
  GraphCommit,
  RepoClient,
  StashEntry,
  StatusEntry,
} from "../ipc/RepoClient";
```

Rename the `LOG_LIMIT` constant to `GRAPH_LIMIT` (same value, same position):

```ts
const GRAPH_LIMIT = 300;
```

In the `AppState` interface, replace `log: CommitInfo[];` in place with:

```ts
  commits: GraphCommit[];
```

In the initial `useState` value, replace `log: [],` in place with `commits: [],`.

In `refresh()`, replace:
```ts
      const [status, log, branches, stashes] = await Promise.all([
        client.getStatus(),
        client.getLog(LOG_LIMIT),
        client.listBranches(),
        client.listStashes(),
      ]);
      setState((prev) => ({ ...prev, status, log, branches, stashes, error: null }));
```
with:
```ts
      const [status, commits, branches, stashes] = await Promise.all([
        client.getStatus(),
        client.getCommitGraph(GRAPH_LIMIT),
        client.listBranches(),
        client.listStashes(),
      ]);
      setState((prev) => ({ ...prev, status, commits, branches, stashes, error: null }));
```

- [ ] **Step 4: Update `useAppState.test.ts`**

Replace `CommitInfo` with `GraphCommit` in the type-only import:

```ts
import type { BranchInfo, GraphCommit, RepoClient, StashEntry, StatusEntry } from "../ipc/RepoClient";
```

In the first test block (`"openRepo populates status and log and sets repoPath"`), replace the
`const commit: CommitInfo = {...}` fixture and rename the test:

```ts
  it("openRepo populates status and commits and sets repoPath", async () => {
    const entry: StatusEntry = { path: "a.txt", staged: false, kind: "Modified" };
    const graphCommit: GraphCommit = {
      id: "abc123",
      shortId: "abc123",
      summary: "initial commit",
      authorName: "Author",
      authorEmail: "author@example.com",
      timestamp: 0,
      parentIds: [],
      branchRefs: [],
    };
```

(Only the fixture's declaration and type change here — the rest of that test block's body is
addressed by the mechanical rename below.)

In **every one of the 17 pre-existing test blocks**, rename the `getLog: async () => X,` line to
`getCommitGraph: async () => X,`, keeping the value `X` exactly as it already is — 16 of the 17
have `X` as `[]` or `unimplemented()` unchanged; the first block's `X` changes from `[commit]` to
`[graphCommit]` (matching the fixture rename in Step 4 above). `getLog`'s line position (right
after `getStatus`, before `listBranches`) is where `getCommitGraph` goes too — same position in
every block, purely a key (and, in one block, value-variable-name) rename.

Replace the one `state.log.length` assertion with `state.commits.length`:

```ts
    expect(result.current.state.commits.length).toBe(1);
```

- [ ] **Step 5: Update `DiffPane.test.tsx`'s and `RepoPicker.test.tsx`'s `fakeClient` factories**

In `frontend/src/components/DiffPane.test.tsx`, replace `getLog: unused,` in place with
`getCommitGraph: unused,`.

In `frontend/src/components/RepoPicker.test.tsx`, replace `getLog: async () =>
unimplemented(),` in place with `getCommitGraph: async () => unimplemented(),`.

- [ ] **Step 6: Run tests, build, and lint**

Run: `cd frontend && pnpm test -- --run && pnpm lint && pnpm build`
Expected: all PASS. `pnpm build` failing here would mean some `RepoClient` implementer was
missed — there should be none left, since this task touched all of them.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/ipc/RepoClient.ts frontend/src/ipc/tauriRepoClient.ts \
  frontend/src/state/useAppState.ts frontend/src/state/useAppState.test.ts \
  frontend/src/components/DiffPane.test.tsx frontend/src/components/RepoPicker.test.tsx
git commit -m "feat(frontend): replace log/CommitInfo with commits/GraphCommit"
```

---

### Task 5: `commitGraphLayout.ts` — lane assignment

**Files:**
- Create: `frontend/src/lib/commitGraphLayout.ts`
- Create: `frontend/src/lib/commitGraphLayout.test.ts`

**Interfaces:**
- Consumes: `GraphCommit` from `../ipc/RepoClient` (Task 4).
- Produces (used by Task 7):
  ```ts
  export interface CommitLayout {
    commitId: string;
    lane: number;
    parentConnections: { parentId: string; lane: number }[];
    passThroughLanes: number[];
  }
  export function assignLanes(commits: GraphCommit[]): CommitLayout[];
  ```
  `passThroughLanes` is the set of lanes that already had an active, unresolved line *before*
  this commit's row was processed (a snapshot taken before this row mutates anything) — the
  renderer (Task 7) uses it to draw each row's upper-half continuation lines, independent of any
  other row's data. No lookahead/lookbehind is needed by the renderer; each row's data is
  self-contained.

- [ ] **Step 1: Write the failing tests**

Create `frontend/src/lib/commitGraphLayout.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import type { GraphCommit } from "../ipc/RepoClient";
import { assignLanes } from "./commitGraphLayout";

function commit(id: string, parentIds: string[]): GraphCommit {
  return {
    id,
    shortId: id,
    summary: id,
    authorName: "Test",
    authorEmail: "test@example.com",
    timestamp: 0,
    parentIds,
    branchRefs: [],
  };
}

describe("assignLanes", () => {
  it("puts every commit in a linear history on lane 0", () => {
    const commits = [commit("C", ["B"]), commit("B", ["A"]), commit("A", [])];

    const layouts = assignLanes(commits);

    expect(layouts.map((l) => l.lane)).toEqual([0, 0, 0]);
  });

  it("opens a second lane for a fork and closes it once the fork tip is placed", () => {
    // F1 and M2 are both children of M1 (a fork); F1 is newer.
    const commits = [commit("F1", ["M1"]), commit("M2", ["M1"]), commit("M1", [])];

    const layouts = assignLanes(commits);

    expect(layouts[0]).toEqual({
      commitId: "F1",
      lane: 0,
      parentConnections: [{ parentId: "M1", lane: 0 }],
      passThroughLanes: [],
    });
    expect(layouts[1]).toEqual({
      commitId: "M2",
      lane: 1,
      parentConnections: [{ parentId: "M1", lane: 0 }],
      passThroughLanes: [0],
    });
    expect(layouts[2].lane).toBe(0);
  });

  it("connects a merge commit's two parents to two different lanes", () => {
    const commits = [commit("M2", ["M1", "F1"]), commit("F1", ["M1"]), commit("M1", [])];

    const layouts = assignLanes(commits);

    expect(layouts[0].lane).toBe(0);
    expect(layouts[0].parentConnections).toEqual([
      { parentId: "M1", lane: 0 },
      { parentId: "F1", lane: 1 },
    ]);
    // F1's own lane (1) closes right after it, since its parent M1 is already tracked in lane 0
    // (opened by M2's first-parent connection) — F1 doesn't need to keep waiting for anything.
    expect(layouts[1]).toEqual({
      commitId: "F1",
      lane: 1,
      parentConnections: [{ parentId: "M1", lane: 0 }],
      passThroughLanes: [0, 1],
    });
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm test -- --run commitGraphLayout`
Expected: FAIL — `./commitGraphLayout` doesn't exist yet.

- [ ] **Step 3: Write the implementation**

Create `frontend/src/lib/commitGraphLayout.ts`:

```ts
import type { GraphCommit } from "../ipc/RepoClient";

export interface CommitLayout {
  commitId: string;
  lane: number;
  parentConnections: { parentId: string; lane: number }[];
  passThroughLanes: number[];
}

export function assignLanes(commits: GraphCommit[]): CommitLayout[] {
  // `lanes[i]` holds the commit id lane `i` is currently waiting to display next, or `null` if
  // that lane is free and its slot can be reused by an unrelated later commit.
  const lanes: (string | null)[] = [];
  const layouts: CommitLayout[] = [];

  const claimLane = (id: string): number => {
    const existing = lanes.indexOf(id);
    if (existing !== -1) {
      return existing;
    }
    const free = lanes.indexOf(null);
    if (free !== -1) {
      lanes[free] = id;
      return free;
    }
    lanes.push(id);
    return lanes.length - 1;
  };

  for (const commit of commits) {
    const passThroughLanes = lanes
      .map((id, i) => (id !== null ? i : -1))
      .filter((i) => i !== -1);

    const lane = claimLane(commit.id);

    const parentConnections: { parentId: string; lane: number }[] = [];
    let laneStillNeeded = false;

    commit.parentIds.forEach((parentId, parentIndex) => {
      const alreadyWaiting = lanes.indexOf(parentId) !== -1;
      if (parentIndex === 0 && !alreadyWaiting) {
        // Straight continuation: this commit's own lane keeps going, now waiting for its
        // first parent — the common case for most rows in a mostly-linear history.
        lanes[lane] = parentId;
        laneStillNeeded = true;
        parentConnections.push({ parentId, lane });
      } else {
        // Either a later (merge) parent, or a first parent some other lane is already
        // waiting for (this commit is where two lanes converge) — either way, this
        // connects to a lane other than the commit's own.
        const parentLane = claimLane(parentId);
        parentConnections.push({ parentId, lane: parentLane });
      }
    });

    // A root commit, or a commit whose first parent turned out to already be tracked
    // elsewhere, has nothing left for its own lane to continue waiting for — free it so a
    // later, unrelated commit can reuse the slot instead of lanes growing forever.
    if (!laneStillNeeded) {
      lanes[lane] = null;
    }

    layouts.push({ commitId: commit.id, lane, parentConnections, passThroughLanes });
  }

  return layouts;
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && pnpm test -- --run commitGraphLayout`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/commitGraphLayout.ts frontend/src/lib/commitGraphLayout.test.ts
git commit -m "feat(frontend): add commit graph lane-assignment algorithm"
```

---

### Task 6: Rename `HistoryList` to `CommitGraph` (behavior-preserving, no visuals yet)

**Files:**
- Create: `frontend/src/components/CommitGraph.tsx` (content moved from `HistoryList.tsx`)
- Create: `frontend/src/components/CommitGraph.test.tsx` (content moved from
  `HistoryList.test.tsx`)
- Delete: `frontend/src/components/HistoryList.tsx`
- Delete: `frontend/src/components/HistoryList.test.tsx`
- Modify: `frontend/src/App.tsx`

**Interfaces:**
- Consumes: `GraphCommit` from Task 4.
- Produces (used by Tasks 7–8): the `CommitGraph` component, same behavior as `HistoryList` had,
  with `log: CommitInfo[]` renamed to `commits: GraphCommit[]` — no other prop changes.

This task is deliberately scoped to the rename and type change only — **no lane graphics or
branch badges yet** (Tasks 7–8). Splitting the rename from the visual addition means a reviewer
can independently verify "the rename didn't silently break anything" before "now it also draws a
graph" — the two are separable claims a reviewer could accept or reject on their own.

- [ ] **Step 1: Write the failing tests**

Create `frontend/src/components/CommitGraph.test.tsx` with the exact content of
`HistoryList.test.tsx`, with these changes:
- `import { HistoryList } from "./HistoryList";` → `import { CommitGraph } from
  "./CommitGraph";`
- `import type { CommitInfo, StashEntry, StatusEntry } from "../ipc/RepoClient";` → `import type
  { GraphCommit, StashEntry, StatusEntry } from "../ipc/RepoClient";`
- The `const log: CommitInfo[] = [...]` fixture becomes `const commits: GraphCommit[] = [...]`,
  with `parentIds: [], branchRefs: [],` added to each of its two entries:
  ```ts
  const commits: GraphCommit[] = [
    {
      id: "aaa111...",
      shortId: "aaa1111",
      summary: "second commit",
      authorName: "Rene",
      authorEmail: "rene@example.com",
      timestamp: 2,
      parentIds: [],
      branchRefs: [],
    },
    {
      id: "bbb222...",
      shortId: "bbb2222",
      summary: "first commit",
      authorName: "Rene",
      authorEmail: "rene@example.com",
      timestamp: 1,
      parentIds: [],
      branchRefs: [],
    },
  ];
  ```
- Every `<HistoryList ... log={log} ... />` becomes `<CommitGraph ... commits={commits} ... />`
  (component name and the one prop name change; every other prop — `status`, `stashes`,
  `selectedRow`, `pending`, `onSelectRow`, `onBranchFromCommit`, `onApplyStash`, `onDropStash` —
  is unchanged; there are 9 such render calls in this file, all needing the same two renames).
- Every other line (test names, assertions, the `stashes` fixture, `fireEvent` calls) is
  unchanged.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm test -- --run CommitGraph`
Expected: FAIL — `./CommitGraph` doesn't exist yet.

- [ ] **Step 3: Write the implementation**

Create `frontend/src/components/CommitGraph.tsx` with the exact content of `HistoryList.tsx`,
with these changes:
- `import type { CommitInfo, StashEntry, StatusEntry } from "../ipc/RepoClient";` → `import type
  { GraphCommit, StashEntry, StatusEntry } from "../ipc/RepoClient";`
- `export function HistoryList({` → `export function CommitGraph({`
- The `log,` destructured prop and its type `log: CommitInfo[];` become `commits,` and `commits:
  GraphCommit[];`.
- `...log.map((commit) => ({ commitId: commit.id })),` → `...commits.map((commit) => ({
  commitId: commit.id })),`
- `{log.map((commit) => (` → `{commits.map((commit) => (` (the commit-row render block itself —
  `{commit.shortId} {commit.summary}` inside the `<li>` — is otherwise byte-for-byte unchanged;
  this is exactly the text the hard backward-compatibility constraint protects, and this task
  doesn't touch it beyond the source variable rename).

Full file for reference (everything else — `rowsEqual`, the stash-row block, the context menu,
keyboard handling — is copied verbatim from `HistoryList.tsx`):

```tsx
import { useState, type KeyboardEvent, type MouseEvent } from "react";
import type { GraphCommit, StashEntry, StatusEntry } from "../ipc/RepoClient";
import type { SelectedRow } from "../state/useAppState";

function rowsEqual(a: SelectedRow, b: SelectedRow): boolean {
  if (a === "uncommitted" || b === "uncommitted") {
    return a === b;
  }
  return a.commitId === b.commitId;
}

export function CommitGraph({
  status,
  commits,
  stashes,
  selectedRow,
  pending,
  onSelectRow,
  onBranchFromCommit,
  onApplyStash,
  onDropStash,
}: {
  status: StatusEntry[];
  commits: GraphCommit[];
  stashes: StashEntry[];
  selectedRow: SelectedRow;
  pending: boolean;
  onSelectRow: (row: SelectedRow) => void;
  onBranchFromCommit: (commitId: string) => void;
  onApplyStash: (index: number) => void;
  onDropStash: (index: number) => void;
}) {
  const [contextMenu, setContextMenu] = useState<{
    commitId: string;
    x: number;
    y: number;
  } | null>(null);

  const rows: SelectedRow[] = [
    "uncommitted",
    ...stashes.map((stash) => ({ commitId: stash.commitId })),
    ...commits.map((commit) => ({ commitId: commit.id })),
  ];
  const selectedIndex = rows.findIndex((row) => rowsEqual(row, selectedRow));

  const handleKeyDown = (event: KeyboardEvent<HTMLUListElement>) => {
    if (event.key === "ArrowDown" || event.key === "j") {
      event.preventDefault();
      onSelectRow(rows[Math.min(selectedIndex + 1, rows.length - 1)]);
    } else if (event.key === "ArrowUp" || event.key === "k") {
      event.preventDefault();
      onSelectRow(rows[Math.max(selectedIndex - 1, 0)]);
    }
  };

  const handleContextMenu = (event: MouseEvent, commitId: string) => {
    event.preventDefault();
    setContextMenu({ commitId, x: event.clientX, y: event.clientY });
  };

  return (
    <ul onKeyDown={handleKeyDown} tabIndex={0}>
      <li
        aria-selected={selectedRow === "uncommitted"}
        onClick={() => onSelectRow("uncommitted")}
      >
        Uncommitted Changes{status.length > 0 && ` (${status.length})`}
      </li>
      {stashes.map((stash) => (
        <li
          key={stash.commitId}
          className="stash-row"
          aria-selected={
            typeof selectedRow === "object" && selectedRow.commitId === stash.commitId
          }
          onClick={() => onSelectRow({ commitId: stash.commitId })}
        >
          <span>{stash.message}</span>
          <button
            disabled={pending}
            onClick={(event: MouseEvent<HTMLButtonElement>) => {
              event.stopPropagation();
              onApplyStash(stash.index);
            }}
          >
            Apply
          </button>
          <button
            disabled={pending}
            onClick={(event: MouseEvent<HTMLButtonElement>) => {
              event.stopPropagation();
              onDropStash(stash.index);
            }}
          >
            Drop
          </button>
        </li>
      ))}
      {commits.map((commit) => (
        <li
          key={commit.id}
          aria-selected={
            typeof selectedRow === "object" && selectedRow.commitId === commit.id
          }
          onClick={() => onSelectRow({ commitId: commit.id })}
          onContextMenu={(event) => handleContextMenu(event, commit.id)}
        >
          {commit.shortId} {commit.summary}
        </li>
      ))}
      {contextMenu !== null && (
        <ul
          style={{ position: "fixed", top: contextMenu.y, left: contextMenu.x }}
          onMouseLeave={() => setContextMenu(null)}
        >
          <li>
            <button
              onClick={() => {
                onBranchFromCommit(contextMenu.commitId);
                setContextMenu(null);
              }}
            >
              Branch from here
            </button>
          </li>
        </ul>
      )}
    </ul>
  );
}
```

Delete `frontend/src/components/HistoryList.tsx` and `frontend/src/components/HistoryList.test.tsx`.

In `frontend/src/App.tsx`, update the import and usage:

```tsx
import { CommitGraph } from "./components/CommitGraph";
```

(replacing `import { HistoryList } from "./components/HistoryList";`), and:

```tsx
        <CommitGraph
          status={appState.state.status}
          commits={appState.state.commits}
          stashes={appState.state.stashes}
          selectedRow={appState.state.selectedRow}
          pending={appState.state.pending}
          onSelectRow={appState.selectRow}
          onBranchFromCommit={appState.openCreateBranchDraft}
          onApplyStash={appState.applyStash}
          onDropStash={appState.dropStash}
        />
```

(replacing the `<HistoryList ... log={appState.state.log} ... />` element — only the component
name and the `log`/`commits` prop change; every other prop is identical.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && pnpm test -- --run CommitGraph && pnpm lint && pnpm build`
Expected: PASS (14 tests, matching `HistoryList.test.tsx`'s prior count exactly — same
assertions, new name), lint clean, build clean.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/CommitGraph.tsx frontend/src/components/CommitGraph.test.tsx \
  frontend/src/App.tsx
git rm frontend/src/components/HistoryList.tsx frontend/src/components/HistoryList.test.tsx
git commit -m "refactor(frontend): rename HistoryList to CommitGraph"
```

---

### Task 7: `CommitLaneGraphic` component

**Files:**
- Create: `frontend/src/components/CommitLaneGraphic.tsx`
- Create: `frontend/src/components/CommitLaneGraphic.test.tsx`

**Interfaces:**
- Consumes: `CommitLayout` from `../lib/commitGraphLayout` (Task 5).
- Produces (used by Task 8): the `CommitLaneGraphic` component.
  ```ts
  { layout: CommitLayout; totalLanes: number }
  ```
  Renders one row's lane graphic: a dot at the commit's own lane, vertical continuation lines
  for the upper half (one per `passThroughLanes` entry) and lower half (one per
  `parentConnections` entry, plus any `passThroughLanes` entry not already covered by a
  connection and not the commit's own lane — a lane just passing through this row untouched).

- [ ] **Step 1: Write the failing tests**

Create `frontend/src/components/CommitLaneGraphic.test.tsx`:

```tsx
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { CommitLayout } from "../lib/commitGraphLayout";
import { CommitLaneGraphic } from "./CommitLaneGraphic";

describe("CommitLaneGraphic", () => {
  it("renders a dot at the commit's lane position", () => {
    const layout: CommitLayout = {
      commitId: "a",
      lane: 1,
      parentConnections: [],
      passThroughLanes: [],
    };

    const { container } = render(<CommitLaneGraphic layout={layout} totalLanes={2} />);

    const circle = container.querySelector("circle");
    expect(circle).not.toBeNull();
    expect(circle?.getAttribute("cx")).toBe(String(1 * 16 + 8));
  });

  it("renders one line per pass-through lane and one per parent connection", () => {
    const layout: CommitLayout = {
      commitId: "a",
      lane: 0,
      parentConnections: [
        { parentId: "b", lane: 0 },
        { parentId: "c", lane: 1 },
      ],
      passThroughLanes: [0],
    };

    const { container } = render(<CommitLaneGraphic layout={layout} totalLanes={2} />);

    const lines = container.querySelectorAll("line");
    // 1 upper pass-through line (lane 0) + 2 lower connector lines (to lane 0 and lane 1).
    expect(lines.length).toBe(3);
  });

  it("sizes the svg to totalLanes", () => {
    const layout: CommitLayout = {
      commitId: "a",
      lane: 0,
      parentConnections: [],
      passThroughLanes: [],
    };

    const { container } = render(<CommitLaneGraphic layout={layout} totalLanes={3} />);

    const svg = container.querySelector("svg");
    expect(svg?.getAttribute("width")).toBe(String(3 * 16));
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm test -- --run CommitLaneGraphic`
Expected: FAIL — `./CommitLaneGraphic` doesn't exist yet.

- [ ] **Step 3: Write the implementation**

Create `frontend/src/components/CommitLaneGraphic.tsx`:

```tsx
import type { CommitLayout } from "../lib/commitGraphLayout";

const LANE_WIDTH = 16;
const ROW_HEIGHT = 24;
const LANE_COLORS = ["#e36209", "#1a7f37", "#0969da", "#8250df", "#cf222e", "#bf8700"];

function laneColor(lane: number): string {
  return LANE_COLORS[lane % LANE_COLORS.length];
}

function laneCenterX(lane: number): number {
  return lane * LANE_WIDTH + LANE_WIDTH / 2;
}

export function CommitLaneGraphic({
  layout,
  totalLanes,
}: {
  layout: CommitLayout;
  totalLanes: number;
}) {
  const width = totalLanes * LANE_WIDTH;
  const midY = ROW_HEIGHT / 2;

  const upperLines = layout.passThroughLanes.map((lane) => (
    <line
      key={`up-${lane}`}
      x1={laneCenterX(lane)}
      y1={0}
      x2={laneCenterX(lane)}
      y2={midY}
      stroke={laneColor(lane)}
      strokeWidth={2}
    />
  ));

  const connectionLines = layout.parentConnections.map((conn) => (
    <line
      key={`down-${conn.lane}`}
      x1={laneCenterX(layout.lane)}
      y1={midY}
      x2={laneCenterX(conn.lane)}
      y2={ROW_HEIGHT}
      stroke={laneColor(layout.lane)}
      strokeWidth={2}
    />
  ));

  const untouchedPassThroughLines = layout.passThroughLanes
    .filter(
      (lane) =>
        lane !== layout.lane && !layout.parentConnections.some((conn) => conn.lane === lane),
    )
    .map((lane) => (
      <line
        key={`through-${lane}`}
        x1={laneCenterX(lane)}
        y1={midY}
        x2={laneCenterX(lane)}
        y2={ROW_HEIGHT}
        stroke={laneColor(lane)}
        strokeWidth={2}
      />
    ));

  return (
    <svg width={width} height={ROW_HEIGHT} style={{ flexShrink: 0 }}>
      {upperLines}
      {connectionLines}
      {untouchedPassThroughLines}
      <circle cx={laneCenterX(layout.lane)} cy={midY} r={4} fill={laneColor(layout.lane)} />
    </svg>
  );
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && pnpm test -- --run CommitLaneGraphic`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/CommitLaneGraphic.tsx frontend/src/components/CommitLaneGraphic.test.tsx
git commit -m "feat(frontend): add CommitLaneGraphic component"
```

---

### Task 8: Wire lane graphics + branch badges into `CommitGraph`

**Files:**
- Modify: `frontend/src/components/CommitGraph.tsx`
- Modify: `frontend/src/components/CommitGraph.test.tsx`
- Modify: `frontend/src/index.css`

**Interfaces:**
- Consumes: `assignLanes`/`CommitLayout` from Task 5, `CommitLaneGraphic` from Task 7.
- Produces: the commit-row visuals the whole feature exists for — nothing further downstream
  depends on this task's own exports (App.tsx already renders `CommitGraph` as of Task 6).

This is the task the design spec's **hard backward-compatibility constraint** binds most
directly: it changes commit-row markup, so it must keep the row an `<li>` whose text content
still contains `{shortId} {summary}` as a plain substring, `aria-selected` still set on that same
`<li>`, and must be verified against the full existing E2E suite, not just this task's own new
tests.

- [ ] **Step 1: Write the failing tests**

In `frontend/src/components/CommitGraph.test.tsx`, append:

```tsx
  it("renders a branch badge for a commit that is a branch tip", () => {
    const commitsWithBranch: GraphCommit[] = [
      { ...commits[0], branchRefs: ["main"] },
      commits[1],
    ];
    render(
      <CommitGraph
        status={status}
        commits={commitsWithBranch}
        stashes={[]}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onApplyStash={vi.fn()}
        onDropStash={vi.fn()}
      />,
    );

    expect(screen.getByText("main")).toBeInTheDocument();
  });

  it("renders a lane graphic for every commit row", () => {
    const { container } = render(
      <CommitGraph
        status={status}
        commits={commits}
        stashes={[]}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onApplyStash={vi.fn()}
        onDropStash={vi.fn()}
      />,
    );

    expect(container.querySelectorAll("li.commit-row svg").length).toBe(commits.length);
  });

  it("still renders each commit's short id and summary as plain text in its own li (hard E2E compatibility constraint)", () => {
    render(
      <CommitGraph
        status={status}
        commits={commits}
        stashes={[]}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onApplyStash={vi.fn()}
        onDropStash={vi.fn()}
      />,
    );

    const row = screen.getByText(/second commit/).closest("li");
    expect(row).not.toBeNull();
    expect(row?.tagName).toBe("LI");
    expect(row?.textContent).toContain("aaa1111");
    expect(row?.textContent).toContain("second commit");
  });
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm test -- --run CommitGraph`
Expected: FAIL — no lane graphic, no branch badges, no `commit-row` class exist yet.

- [ ] **Step 3: Write the implementation**

In `frontend/src/components/CommitGraph.tsx`, add the imports:

```tsx
import { assignLanes } from "../lib/commitGraphLayout";
import { CommitLaneGraphic } from "./CommitLaneGraphic";
```

Inside the component body, before the `return`, compute the layouts and total lane count:

```tsx
  const commitLayouts = assignLanes(commits);
  const laneCount =
    Math.max(
      0,
      ...commitLayouts.map((l) => l.lane),
      ...commitLayouts.flatMap((l) => l.passThroughLanes),
      ...commitLayouts.flatMap((l) => l.parentConnections.map((c) => c.lane)),
    ) + 1;
```

Replace the commit-row `{commits.map((commit) => ( ... ))}` block with:

```tsx
      {commits.map((commit, index) => (
        <li
          key={commit.id}
          className="commit-row"
          aria-selected={
            typeof selectedRow === "object" && selectedRow.commitId === commit.id
          }
          onClick={() => onSelectRow({ commitId: commit.id })}
          onContextMenu={(event) => handleContextMenu(event, commit.id)}
        >
          <CommitLaneGraphic layout={commitLayouts[index]} totalLanes={laneCount} />
          {commit.branchRefs.map((ref) => (
            <span key={ref} className="branch-badge">
              {ref}
            </span>
          ))}
          <span className="commit-summary">
            {commit.shortId} {commit.summary}
          </span>
        </li>
      ))}
```

In `frontend/src/index.css`, add (after the existing `.stash-row`/`.stash-row > span` rules,
before the `[aria-selected="true"]` rule — same reasoning as `.stash-row`: this row has non-text
children, so the base `.app-layout > ul > li` rule's `white-space: nowrap; overflow: hidden`
would clip them for a long summary, exactly the class of bug the stash feature already hit once):

```css
/* Same trap as `.stash-row` above: this row has an SVG lane graphic and branch badges as
   children, not just text — the base rule's nowrap+hidden would clip them for a long commit
   summary. Flex layout instead, truncation confined to the summary `<span>`. */
.app-layout > ul > li.commit-row {
  display: flex;
  align-items: center;
  gap: 4px;
  white-space: normal;
  overflow: hidden;
}

.app-layout > ul > li.commit-row > span.commit-summary {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
  flex: 1 1 auto;
}

.branch-badge {
  font-size: 11px;
  padding: 1px 6px;
  border-radius: 10px;
  background: var(--hunk-bg);
  color: var(--text-muted);
  border: 1px solid var(--border);
  flex-shrink: 0;
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && pnpm test -- --run && pnpm lint && pnpm build`
Expected: PASS, full suite (17 tests in `CommitGraph.test.tsx`: 14 from Task 6 + 3 new), lint
clean, build clean.

- [ ] **Step 5: Manually verify in the running app**

Run: `cargo tauri dev`
Expected: the commit list now shows colored lane lines and dots per row; branch-tip commits show
a small badge with the branch name; clicking/keyboard-navigating rows still works exactly as
before.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/CommitGraph.tsx frontend/src/components/CommitGraph.test.tsx \
  frontend/src/index.css
git commit -m "feat(frontend): render lane graphics and branch badges in CommitGraph"
```

---

### Task 9: E2E flow — multi-branch graph, plus full regression of the existing suite

**Files:**
- Create: `e2e/specs/commit-graph.spec.ts`

**Interfaces:**
- Consumes: the built app from Tasks 1–8, driven as a black box via `tauri-driver` +
  WebdriverIO (same harness as the other `e2e/specs/*.spec.ts` files).

**Design notes carried over from the three prior features' E2E postmortems:**
- WebdriverIO loads spec files alphabetically. `"commit-graph.spec.ts"` sorts between
  `"branch-management.spec.ts"` and `"first-flow.spec.ts"` — don't rely on that order; this spec
  is self-sufficient regardless (own fixture commits via direct `git` calls, no assumption about
  what any other spec left behind, including not hardcoding a default branch name like
  "main"/"master" — read the actual current branch via `git rev-parse --abbrev-ref HEAD` instead,
  the same lesson `branch-management.spec.ts` already learned).
- The app has no polling. A fixture committed via raw `git` calls (outside the app) is invisible
  to the already-rendered DOM until some UI-driven mutation triggers `useAppState`'s `refresh()` —
  this spec creates a throwaway uncommitted `prime.txt`, and the `it()` block's first steps
  stage+commit it through the real UI, which is what makes the fixture's other, already-committed
  history visible.
- **This is also the task that gives the hard backward-compatibility constraint its real,
  binding verification**: the full existing E2E suite must still pass alongside this new spec,
  not just this task's own new assertions.

- [ ] **Step 1: Write the E2E spec**

Create `e2e/specs/commit-graph.spec.ts`:

```ts
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");

describe("Browsitory commit graph", () => {
  before(() => {
    const fixturePath = path.join(E2E_REPO_PATH, "graph-fixture.txt");
    fs.writeFileSync(fixturePath, "base\n");
    execFileSync("git", ["add", "graph-fixture.txt"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: commit graph base commit"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });

    const baseBranch = execFileSync("git", ["rev-parse", "--abbrev-ref", "HEAD"], {
      cwd: E2E_REPO_PATH,
    })
      .toString()
      .trim();

    execFileSync("git", ["checkout", "-b", "e2e-graph-feature"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });
    fs.writeFileSync(path.join(E2E_REPO_PATH, "feature-fixture.txt"), "feature\n");
    execFileSync("git", ["add", "feature-fixture.txt"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: commit graph feature commit"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });

    execFileSync("git", ["checkout", baseBranch], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    fs.writeFileSync(path.join(E2E_REPO_PATH, "prime.txt"), "prime\n");
  });

  it("shows commits from every local branch with correct branch labels", async () => {
    const commitMessageInput = await $("textarea[placeholder='Commit message']");
    await commitMessageInput.waitForExist({ timeout: 10000 });

    const stageButton = await $("button=Stage");
    await stageButton.click();
    await commitMessageInput.setValue("e2e: prime the refresh");
    const commitButton = await $("button=Commit");
    await commitButton.click();

    const baseCommitEntry = await $("li*=e2e: commit graph base commit");
    await baseCommitEntry.waitForExist({ timeout: 10000 });
    const featureCommitEntry = await $("li*=e2e: commit graph feature commit");
    await featureCommitEntry.waitForExist({ timeout: 10000 });

    // The feature branch's tip commit should carry a branch-name badge in the same row —
    // confirms `branch_refs` made it end-to-end from git-core through to the rendered row.
    const featureCommitWithBadge = await $("li*=e2e-graph-feature");
    await expect(featureCommitWithBadge).toBeExisting();
  });
});
```

- [ ] **Step 2: Build and run the full suite**

Run (from repo root, per `CLAUDE.md`'s E2E sequence):
```bash
cd frontend && VITE_E2E_REPO_PATH=/tmp/browsitory-e2e-repo pnpm build && cd ..
cargo build --workspace --features tauri-app/custom-protocol
cd e2e && pnpm install && xvfb-run --auto-servernum pnpm test
```
Expected: `commit-graph.spec.ts` AND all four pre-existing specs (`blame-viewer.spec.ts`,
`branch-management.spec.ts`, `first-flow.spec.ts`, `stash-management.spec.ts`) all PASS — this
is the concrete, non-negotiable check for the hard backward-compatibility constraint. If any
pre-existing spec now fails, that is this task's own regression to fix (most likely a commit-row
markup issue from Task 8), not something to route around.

- [ ] **Step 3: Commit**

```bash
git add e2e/specs/commit-graph.spec.ts
git commit -m "test(e2e): add multi-branch commit graph flow"
```

---

### Task 10: Delete `git-core::log`

**Files:**
- Delete: `crates/git-core/src/log.rs`
- Delete: `crates/git-core/tests/log.rs`
- Modify: `crates/git-core/src/lib.rs`
- Modify: `crates/git-core/src/blame.rs` (one stale comment)

**Interfaces:** none — pure removal, nothing downstream depends on this task.

By this point nothing in the workspace references `git_core::log`/`CommitInfo` — Tasks 2–4
already moved every consumer (`tauri-app`'s worker/commands, `frontend`'s `RepoClient`/
`useAppState`) onto `graph`/`GraphCommit`. Rust doesn't warn or error on a `pub` item in a lib
crate simply going unused by other crates, so this deletion was safe to defer until every caller
had already moved off it, with zero risk of an intermediate broken build at any point before now.

- [ ] **Step 1: Delete the module and its tests**

```bash
git rm crates/git-core/src/log.rs crates/git-core/tests/log.rs
```

Remove `pub mod log;` from `crates/git-core/src/lib.rs`:

```rust
pub mod blame;
pub mod branch;
pub mod commit;
pub mod diff;
pub mod graph;
pub mod repo;
pub mod stage;
pub mod stash;
pub mod status;
```

In `crates/git-core/src/blame.rs`, fix the now-stale comment on `BlameLine.timestamp` (it
referenced the now-deleted `CommitInfo`):

```rust
    pub timestamp: i64, // Unix seconds, UTC — matches GraphCommit's existing convention
```

- [ ] **Step 2: Verify the workspace still builds and all tests pass**

Run: `cargo build --workspace && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Expected: all PASS/clean — confirming nothing still referenced the deleted module.

- [ ] **Step 3: Commit**

```bash
git add crates/git-core/src/lib.rs crates/git-core/src/blame.rs
git commit -m "chore(git-core): remove log module, superseded by graph"
```

---

## Self-Review Notes

- **Spec coverage:** `graph_log` implemented, showing all local branches' commits via
  `push_glob("refs/heads/*")` (Task 1); lane assignment is frontend-only, backend returns pure
  graph structure (Tasks 1 vs. 5 — verified: `GraphCommit` carries no lane/column fields anywhere
  in the Rust or DTO layer); full swimlane rendering, not a flat-badge fallback (Tasks 7–8, real
  SVG lines/curves via a generically-correct lane algorithm, traced by hand against three
  topologies — linear, fork, merge — before being written into the plan's tests); `HistoryList`→
  `CommitGraph` rename preserving every existing behavior (Task 6, all 14 pre-existing assertions
  carried over unchanged); `log`/`CommitInfo`/`getLog` removed once nothing references them (Task
  10, sequenced last specifically because Rust never complains about an unused `pub` item, so
  there was no reason to risk a broken intermediate build by deleting earlier). The hard
  backward-compatibility constraint is enforced concretely, not just stated: Task 8 adds a test
  asserting the commit row is still an `<li>` with `{shortId} {summary}` as plain text content,
  and Task 9 re-runs the full pre-existing E2E suite, not just the new spec. All covered.
- **Placeholder scan:** none found — every step has real code, real test bodies (including a
  hand-traced, verified-correct lane-assignment algorithm, not hand-waved), real commands.
- **Type consistency:** `GraphCommit { id, shortId, summary, authorName, authorEmail, timestamp,
  parentIds, branchRefs }` used identically from Task 4 through Task 9. `CommitLayout {
  commitId, lane, parentConnections, passThroughLanes }` used identically from Task 5 through
  Task 8, and `CommitLaneGraphic`'s prop shape (Task 7) matches exactly what Task 8 passes it.
  `assignLanes(commits: GraphCommit[]): CommitLayout[]` signature matches its Task 5 definition
  and Task 8's call site exactly.
- **Task-sizing judgment call, stated explicitly rather than left implicit:** Task 4 is
  noticeably larger than a typical single task in this plan's style, because removing (not
  adding) an interface method breaks every consumer simultaneously, including `useAppState.ts`
  itself — there is no valid, independently-reviewable intermediate state between "RepoClient
  still has getLog" and "RepoClient doesn't, and neither does anything that called it." Splitting
  it further would only create a broken commit in between, the exact class of issue the blame
  feature's final review flagged (two commits failing `tsc` because a required prop was added in
  one task and consumed three commits later in another) — this plan avoids repeating that by
  keeping genuinely-inseparable changes in one task instead of forcing an artificial split.
