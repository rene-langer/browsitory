# Branch Management Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add local branch management (list, create, switch, delete, rename) to Browsitory, per
`docs/superpowers/specs/2026-08-12-branch-management-design.md`.

**Architecture:** A new `git-core::branch` module (git2, real-repo-tested, no mocks) exposes the
five operations; `tauri-app`'s existing `Worker`/`Command`/Tauri-command layering is extended with
five new pass-through commands; the frontend's `RepoClient` interface, `useAppState` hook, and two
components (`BranchSwitcher`, `HistoryList`) grow to match. No new architectural layer — this
follows the exact `git-core` → `Worker` → Tauri command → `RepoClient` → component chain every
Phase 1 feature already uses.

**Tech Stack:** Rust (git2, thiserror), Tauri 2, React/TypeScript, Vitest + Testing Library,
`tauri-driver` + WebdriverIO for E2E.

## Global Constraints

- Local branches only — no remote-tracking branches, no push/pull (spec's stated non-goal).
- No stash integration for dirty-tree switches — switch failure surfaces as a plain error.
- Rely on git2's own safe-checkout error for dirty-tree switch detection — no frontend
  pre-check via `getStatus()`.
- Branch state lives in the existing `useAppState` hook — no separate branch store/hook.
- Delete blocks by default on unmerged commits; force-delete is an explicit, scoped retry, not
  a generic force-everything toggle.
- Creating a branch always auto-switches to it; no separate flag.
- `git-core` tests use real temp-dir repos (`crates/git-core/tests/common/mod.rs` helpers), never
  mocks. `tauri-app` tests use real temp-dir repos too, inline in the module they test.
  `frontend` tests mock `RepoClient`, never `@tauri-apps/api`.
- Test commands: `cargo test -p git-core --test branch`, `cargo test -p tauri-app`,
  `cd frontend && pnpm test -- --run`, `cd e2e && pnpm test` (per `CLAUDE.md`'s E2E setup
  sequence — needs a fresh `pnpm build` + `cargo build --workspace --features
  tauri-app/custom-protocol` first).

---

### Task 1: `git-core::branch` — list, create, switch, rename

**Files:**
- Create: `crates/git-core/src/branch.rs`
- Modify: `crates/git-core/src/lib.rs` (add `pub mod branch;`)
- Test: `crates/git-core/tests/branch.rs`

**Interfaces:**
- Consumes: `crates/git-core/tests/common/mod.rs`'s `init_repo()`, `commit_all()`, `write_file()`
  (already exist, unchanged).
- Produces (used by Task 2 and Task 3):
  ```rust
  pub struct BranchInfo { pub name: String, pub is_current: bool }
  pub enum BranchError { Git(git2::Error), NotMerged(String) }
  pub fn list_branches(repo: &git2::Repository) -> Result<Vec<BranchInfo>, BranchError>;
  pub fn create_branch(repo: &git2::Repository, name: &str, start_point: &str) -> Result<(), BranchError>;
  pub fn switch_branch(repo: &git2::Repository, name: &str) -> Result<(), BranchError>;
  pub fn rename_branch(repo: &git2::Repository, old_name: &str, new_name: &str) -> Result<(), BranchError>;
  ```
  (`delete_branch` is added in Task 2, alongside `BranchError::NotMerged`'s only producer.)

- [ ] **Step 1: Write the failing tests**

Create `crates/git-core/tests/branch.rs`:

```rust
mod common;

use common::{commit_all, init_repo, write_file};

#[test]
fn list_branches_reports_the_current_branch() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "content");
    commit_all(&repo, "initial commit");

    let branches = git_core::branch::list_branches(&repo).unwrap();

    assert_eq!(branches.len(), 1);
    assert!(branches[0].is_current);
}

#[test]
fn create_branch_from_head_adds_and_switches_to_it() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "content");
    commit_all(&repo, "initial commit");

    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();

    let branches = git_core::branch::list_branches(&repo).unwrap();
    assert_eq!(branches.len(), 2);
    let feature = branches.iter().find(|b| b.name == "feature").unwrap();
    assert!(feature.is_current);
    assert_eq!(branches.iter().filter(|b| b.is_current).count(), 1);
}

#[test]
fn create_branch_from_a_specific_commit_uses_that_commit_as_start_point() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "first commit");
    let first_commit_id = repo.head().unwrap().peel_to_commit().unwrap().id().to_string();
    write_file(dir.path(), "file.txt", "v2");
    commit_all(&repo, "second commit");

    git_core::branch::create_branch(&repo, "from-first", &first_commit_id).unwrap();

    let branch = repo.find_branch("from-first", git2::BranchType::Local).unwrap();
    let branch_commit = branch.get().peel_to_commit().unwrap();
    assert_eq!(branch_commit.id().to_string(), first_commit_id);
}

#[test]
fn switch_branch_moves_head_and_updates_the_working_tree() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let initial_branch = git_core::branch::list_branches(&repo).unwrap()[0].name.clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "file.txt", "v2-on-feature");
    commit_all(&repo, "feature commit");

    git_core::branch::switch_branch(&repo, &initial_branch).unwrap();

    let contents = std::fs::read_to_string(dir.path().join("file.txt")).unwrap();
    assert_eq!(contents, "v1");
    let branches = git_core::branch::list_branches(&repo).unwrap();
    assert!(
        branches
            .iter()
            .find(|b| b.name == initial_branch)
            .unwrap()
            .is_current
    );
}

#[test]
fn switch_branch_is_blocked_by_a_conflicting_dirty_file() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let initial_branch = git_core::branch::list_branches(&repo).unwrap()[0].name.clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "file.txt", "v2-on-feature");
    commit_all(&repo, "feature commit");
    git_core::branch::switch_branch(&repo, &initial_branch).unwrap();
    // Dirty the file with content that differs from both branches' tips — the exact shape of
    // conflict libgit2's safe checkout refuses to silently overwrite.
    write_file(dir.path(), "file.txt", "uncommitted local edit");

    let result = git_core::branch::switch_branch(&repo, "feature");

    assert!(result.is_err());
    let contents = std::fs::read_to_string(dir.path().join("file.txt")).unwrap();
    assert_eq!(contents, "uncommitted local edit");
    assert!(
        git_core::branch::list_branches(&repo)
            .unwrap()
            .iter()
            .find(|b| b.name == initial_branch)
            .unwrap()
            .is_current
    );
}

#[test]
fn rename_branch_updates_head_when_renaming_the_current_branch() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let initial_branch = git_core::branch::list_branches(&repo).unwrap()[0].name.clone();

    git_core::branch::rename_branch(&repo, &initial_branch, "renamed").unwrap();

    let branches = git_core::branch::list_branches(&repo).unwrap();
    assert_eq!(branches.len(), 1);
    assert_eq!(branches[0].name, "renamed");
    assert!(branches[0].is_current);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test branch`
Expected: FAIL to compile — `git_core::branch` doesn't exist yet.

- [ ] **Step 3: Write the implementation**

Create `crates/git-core/src/branch.rs`:

```rust
use git2::{BranchType, Repository};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BranchError {
    #[error("git operation failed: {0}")]
    Git(#[from] git2::Error),
    #[error("branch '{0}' has unmerged commits; use force to delete anyway")]
    NotMerged(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchInfo {
    pub name: String,
    pub is_current: bool,
}

pub fn list_branches(repo: &Repository) -> Result<Vec<BranchInfo>, BranchError> {
    let mut branches = Vec::new();
    for entry in repo.branches(Some(BranchType::Local))? {
        let (branch, _) = entry?;
        let Ok(Some(name)) = branch.name() else {
            continue;
        };
        branches.push(BranchInfo {
            name: name.to_string(),
            is_current: branch.is_head(),
        });
    }
    Ok(branches)
}

fn resolve_start_point<'repo>(
    repo: &'repo Repository,
    start_point: &str,
) -> Result<git2::Commit<'repo>, BranchError> {
    if start_point == "HEAD" {
        Ok(repo.head()?.peel_to_commit()?)
    } else {
        Ok(repo.revparse_single(start_point)?.peel_to_commit()?)
    }
}

pub fn create_branch(repo: &Repository, name: &str, start_point: &str) -> Result<(), BranchError> {
    let commit = resolve_start_point(repo, start_point)?;
    repo.branch(name, &commit, false)?;
    // The branch ref above is created regardless of what happens next — if switch_branch
    // fails (e.g. a dirty-tree conflict), the branch stays created rather than being rolled
    // back, matching `git checkout -b`'s own behavior. See the design spec's note on this.
    switch_branch(repo, name)
}

pub fn switch_branch(repo: &Repository, name: &str) -> Result<(), BranchError> {
    let branch_ref = format!("refs/heads/{name}");
    let target = repo.find_reference(&branch_ref)?.peel_to_commit()?;
    // Checking out the tree before moving HEAD means a refused checkout (libgit2's default
    // "safe" strategy errors rather than overwriting modified/untracked files that differ from
    // the target) leaves the repo exactly as it was — HEAD only moves once the working
    // directory update has already succeeded.
    repo.checkout_tree(target.as_object(), None)?;
    repo.set_head(&branch_ref)?;
    Ok(())
}

pub fn rename_branch(repo: &Repository, old_name: &str, new_name: &str) -> Result<(), BranchError> {
    let mut branch = repo.find_branch(old_name, BranchType::Local)?;
    branch.rename(new_name, false)?;
    Ok(())
}
```

Add `pub mod branch;` to `crates/git-core/src/lib.rs` (alongside the existing five `pub mod`
lines, alphabetically between `pub mod commit;`'s predecessor — the file is a flat alphabetical
list, so `branch` goes first):

```rust
pub mod branch;
pub mod commit;
pub mod diff;
pub mod log;
pub mod repo;
pub mod stage;
pub mod status;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p git-core --test branch`
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/git-core/src/branch.rs crates/git-core/src/lib.rs crates/git-core/tests/branch.rs
git commit -m "feat(git-core): add branch list/create/switch/rename"
```

---

### Task 2: `git-core::branch::delete_branch` — merge-safety check + force

**Files:**
- Modify: `crates/git-core/src/branch.rs`
- Test: `crates/git-core/tests/branch.rs`

**Interfaces:**
- Consumes: `BranchInfo`, `BranchError`, `list_branches`, `create_branch`, `switch_branch` from
  Task 1 (unchanged signatures).
- Produces (used by Task 3):
  ```rust
  pub fn delete_branch(repo: &git2::Repository, name: &str, force: bool) -> Result<(), BranchError>;
  ```

- [ ] **Step 1: Write the failing tests**

Append to `crates/git-core/tests/branch.rs`:

```rust
#[test]
fn delete_branch_without_force_fails_on_an_unmerged_branch() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "file.txt", "v2");
    commit_all(&repo, "feature commit");
    let initial_branch = git_core::branch::list_branches(&repo)
        .unwrap()
        .into_iter()
        .find(|b| b.name != "feature")
        .unwrap()
        .name;
    git_core::branch::switch_branch(&repo, &initial_branch).unwrap();

    let result = git_core::branch::delete_branch(&repo, "feature", false);

    assert!(matches!(result, Err(git_core::branch::BranchError::NotMerged(_))));
    assert!(
        git_core::branch::list_branches(&repo)
            .unwrap()
            .iter()
            .any(|b| b.name == "feature")
    );
}

#[test]
fn delete_branch_with_force_deletes_an_unmerged_branch() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "file.txt", "v2");
    commit_all(&repo, "feature commit");
    let initial_branch = git_core::branch::list_branches(&repo)
        .unwrap()
        .into_iter()
        .find(|b| b.name != "feature")
        .unwrap()
        .name;
    git_core::branch::switch_branch(&repo, &initial_branch).unwrap();

    git_core::branch::delete_branch(&repo, "feature", true).unwrap();

    assert!(
        !git_core::branch::list_branches(&repo)
            .unwrap()
            .iter()
            .any(|b| b.name == "feature")
    );
}

#[test]
fn delete_branch_without_force_succeeds_when_fully_merged() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let initial_branch = git_core::branch::list_branches(&repo).unwrap()[0].name.clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    // "feature" has no commits of its own — its tip exactly matches the initial branch's tip,
    // the edge case libgit2's graph_descendant_of doesn't treat as "descendant" on its own.
    git_core::branch::switch_branch(&repo, &initial_branch).unwrap();

    git_core::branch::delete_branch(&repo, "feature", false).unwrap();

    assert!(
        !git_core::branch::list_branches(&repo)
            .unwrap()
            .iter()
            .any(|b| b.name == "feature")
    );
}

#[test]
fn delete_branch_fails_when_deleting_the_current_branch_even_with_force() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let initial_branch = git_core::branch::list_branches(&repo).unwrap()[0].name.clone();

    let result = git_core::branch::delete_branch(&repo, &initial_branch, true);

    assert!(result.is_err());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test branch`
Expected: FAIL to compile — `delete_branch` doesn't exist yet.

- [ ] **Step 3: Write the implementation**

Add to `crates/git-core/src/branch.rs` (after `rename_branch`):

```rust
pub fn delete_branch(repo: &Repository, name: &str, force: bool) -> Result<(), BranchError> {
    let mut branch = repo.find_branch(name, BranchType::Local)?;
    if !force {
        let branch_oid = branch.get().peel_to_commit()?.id();
        let head_oid = repo.head()?.peel_to_commit()?.id();
        // libgit2's graph_descendant_of treats a commit as not-a-descendant-of-itself, so an
        // exact tip match (the branch is fully caught up with HEAD, no divergent commits)
        // needs its own check to count as merged.
        let merged = branch_oid == head_oid || repo.graph_descendant_of(head_oid, branch_oid)?;
        if !merged {
            return Err(BranchError::NotMerged(name.to_string()));
        }
    }
    branch.delete()?;
    Ok(())
}
```

(Deleting the currently checked-out branch is refused by libgit2 itself, independent of the
`force` flag — `Branch::delete()` surfaces that as a `git2::Error`, wrapped into
`BranchError::Git`. No extra code needed for that case; the last test above exercises it.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p git-core --test branch`
Expected: PASS (10 tests total).

- [ ] **Step 5: Commit**

```bash
git add crates/git-core/src/branch.rs crates/git-core/tests/branch.rs
git commit -m "feat(git-core): add branch delete with unmerged-commit safety check"
```

---

### Task 3: `tauri-app::worker` — Command variants + WorkerHandle methods

**Files:**
- Modify: `crates/tauri-app/src/worker.rs`

**Interfaces:**
- Consumes: `git_core::branch::{BranchInfo, list_branches, create_branch, switch_branch,
  delete_branch, rename_branch}` from Tasks 1–2.
- Produces (used by Task 4):
  ```rust
  impl WorkerHandle {
      pub fn list_branches(&self) -> Result<Vec<BranchInfo>, String>;
      pub fn create_branch(&self, name: String, start_point: String) -> Result<(), String>;
      pub fn switch_branch(&self, name: String) -> Result<(), String>;
      pub fn delete_branch(&self, name: String, force: bool) -> Result<(), String>;
      pub fn rename_branch(&self, old_name: String, new_name: String) -> Result<(), String>;
  }
  ```

- [ ] **Step 1: Write the failing tests**

Add to `crates/tauri-app/src/worker.rs`'s `#[cfg(test)] mod tests` block (after
`stage_then_commit_round_trips_through_the_worker`, reusing that module's existing `init_repo`,
`write_file`, `commit_all` helpers unchanged):

```rust
#[test]
fn list_branches_reflects_the_initial_branch_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let branches = worker.handle().list_branches().unwrap();

    assert_eq!(branches.len(), 1);
    assert!(branches[0].is_current);
}

#[test]
fn create_then_switch_branch_round_trips_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle
        .create_branch("feature".into(), "HEAD".into())
        .unwrap();

    let branches = handle.list_branches().unwrap();
    let feature = branches.iter().find(|b| b.name == "feature").unwrap();
    assert!(feature.is_current);

    let initial_branch_name = branches
        .iter()
        .find(|b| b.name != "feature")
        .unwrap()
        .name
        .clone();
    handle.switch_branch(initial_branch_name.clone()).unwrap();

    let branches_after = handle.list_branches().unwrap();
    assert!(
        branches_after
            .iter()
            .find(|b| b.name == initial_branch_name)
            .unwrap()
            .is_current
    );
}

#[test]
fn rename_branch_round_trips_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let initial_branch_name = handle.list_branches().unwrap()[0].name.clone();

    handle
        .rename_branch(initial_branch_name, "renamed".into())
        .unwrap();

    let branches = handle.list_branches().unwrap();
    assert_eq!(branches[0].name, "renamed");
}

#[test]
fn delete_branch_with_force_round_trips_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle
        .create_branch("feature".into(), "HEAD".into())
        .unwrap();
    let initial_branch_name = handle
        .list_branches()
        .unwrap()
        .into_iter()
        .find(|b| b.name != "feature")
        .unwrap()
        .name;
    handle.switch_branch(initial_branch_name).unwrap();

    handle.delete_branch("feature".into(), true).unwrap();

    assert!(
        !handle
            .list_branches()
            .unwrap()
            .iter()
            .any(|b| b.name == "feature")
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p tauri-app`
Expected: FAIL to compile — `Command::ListBranches` etc. and the `WorkerHandle` methods don't
exist yet.

- [ ] **Step 3: Write the implementation**

In `crates/tauri-app/src/worker.rs`, add the import (alongside the other three `git_core::*`
imports at the top):

```rust
use git_core::branch::BranchInfo;
```

Add five variants to the `Command` enum (after `Commit`):

```rust
    ListBranches {
        reply: Sender<Result<Vec<BranchInfo>, String>>,
    },
    CreateBranch {
        name: String,
        start_point: String,
        reply: Sender<Result<(), String>>,
    },
    SwitchBranch {
        name: String,
        reply: Sender<Result<(), String>>,
    },
    DeleteBranch {
        name: String,
        force: bool,
        reply: Sender<Result<(), String>>,
    },
    RenameBranch {
        old_name: String,
        new_name: String,
        reply: Sender<Result<(), String>>,
    },
```

Add five match arms in the worker thread loop (after the `Command::Commit` arm):

```rust
                    Command::ListBranches { reply } => {
                        let result =
                            git_core::branch::list_branches(&repo).map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::CreateBranch {
                        name,
                        start_point,
                        reply,
                    } => {
                        let result = git_core::branch::create_branch(&repo, &name, &start_point)
                            .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::SwitchBranch { name, reply } => {
                        let result = git_core::branch::switch_branch(&repo, &name)
                            .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::DeleteBranch { name, force, reply } => {
                        let result = git_core::branch::delete_branch(&repo, &name, force)
                            .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::RenameBranch {
                        old_name,
                        new_name,
                        reply,
                    } => {
                        let result =
                            git_core::branch::rename_branch(&repo, &old_name, &new_name)
                                .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
```

Add five methods to `impl WorkerHandle` (after `commit`), following the exact
send-then-block-on-reply pattern every existing method uses:

```rust
    pub fn list_branches(&self) -> Result<Vec<BranchInfo>, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::ListBranches { reply: reply_tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn create_branch(&self, name: String, start_point: String) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::CreateBranch {
                name,
                start_point,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn switch_branch(&self, name: String) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::SwitchBranch {
                name,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn delete_branch(&self, name: String, force: bool) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::DeleteBranch {
                name,
                force,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn rename_branch(&self, old_name: String, new_name: String) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::RenameBranch {
                old_name,
                new_name,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p tauri-app`
Expected: PASS (all existing tests plus the 4 new ones).

- [ ] **Step 5: Commit**

```bash
git add crates/tauri-app/src/worker.rs
git commit -m "feat(tauri-app): wire branch operations through the worker"
```

---

### Task 4: Tauri commands + registration

**Files:**
- Modify: `crates/tauri-app/src/commands.rs`
- Modify: `crates/tauri-app/src/main.rs`

**Interfaces:**
- Consumes: `WorkerHandle::{list_branches, create_branch, switch_branch, delete_branch,
  rename_branch}` from Task 3; `git_core::branch::BranchInfo` from Task 1.
- Produces (used by Task 5):
  - Tauri commands `list_branches`, `create_branch`, `switch_branch`, `delete_branch`,
    `rename_branch`, each returning `Result<T, String>` and reachable via `invoke(...)`.
  - `BranchInfoDto { name: String, is_current: bool }` (serialized `camelCase`: `name`,
    `isCurrent`).

No dedicated test for this task: these are thin pass-through commands (per `CLAUDE.md`'s testing
convention), and `BranchInfoDto` uses plain field serialization, not an enum's `Debug` output —
unlike `StatusKind`/`DiffLineOrigin`, there's no string-formatting drift for a pinned test to
catch. The `git-core`/`Worker` tests from Tasks 1–3 already cover the logic these commands call.

- [ ] **Step 1: Add the DTO and commands**

In `crates/tauri-app/src/commands.rs`, add the DTO (after `CommitInfoDto`'s `impl From` block):

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchInfoDto {
    pub name: String,
    pub is_current: bool,
}

impl From<git_core::branch::BranchInfo> for BranchInfoDto {
    fn from(b: git_core::branch::BranchInfo) -> Self {
        BranchInfoDto {
            name: b.name,
            is_current: b.is_current,
        }
    }
}
```

Add five commands (after the existing `commit` command, before the `#[cfg(test)]` module):

```rust
#[tauri::command]
pub async fn list_branches(state: State<'_, AppState>) -> Result<Vec<BranchInfoDto>, String> {
    let branches = worker_handle(&state)?.list_branches()?;
    Ok(branches.into_iter().map(BranchInfoDto::from).collect())
}

#[tauri::command]
pub async fn create_branch(
    name: String,
    start_point: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state)?.create_branch(name, start_point)
}

#[tauri::command]
pub async fn switch_branch(name: String, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state)?.switch_branch(name)
}

#[tauri::command]
pub async fn delete_branch(
    name: String,
    force: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state)?.delete_branch(name, force)
}

#[tauri::command]
pub async fn rename_branch(
    old_name: String,
    new_name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state)?.rename_branch(old_name, new_name)
}
```

In `crates/tauri-app/src/main.rs`, add the five commands to the `use commands::{...}` import list
and to `tauri::generate_handler![...]`:

```rust
use commands::{
    commit, create_branch, delete_branch, get_commit_diff, get_commit_files, get_log, get_status,
    get_working_diff, list_branches, list_recent_repos, open_repo, pick_repo_folder,
    rename_branch, stage_file, switch_branch, unstage_file, AppState,
};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            open_repo,
            get_status,
            get_log,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 2: Verify it builds**

Run: `cargo build --workspace`
Expected: builds cleanly (no test failures possible for pass-through commands; this step catches
typos/signature mismatches).

- [ ] **Step 3: Run the full test suite**

Run: `cargo test --workspace`
Expected: PASS (all previously-passing tests still pass; nothing new to assert here).

- [ ] **Step 4: Commit**

```bash
git add crates/tauri-app/src/commands.rs crates/tauri-app/src/main.rs
git commit -m "feat(tauri-app): expose branch operations as Tauri commands"
```

---

### Task 5: `RepoClient` + `tauriRepoClient` — frontend IPC layer

**Files:**
- Modify: `frontend/src/ipc/RepoClient.ts`
- Modify: `frontend/src/ipc/tauriRepoClient.ts`

**Interfaces:**
- Consumes: Tauri commands `list_branches`, `create_branch`, `switch_branch`, `delete_branch`,
  `rename_branch` from Task 4 (wire names snake_case; JS-side argument keys camelCase, matching
  every existing `tauriRepoClient.ts` entry).
- Produces (used by Task 6):
  ```ts
  export interface BranchInfo { name: string; isCurrent: boolean; }
  // on RepoClient:
  listBranches(): Promise<BranchInfo[]>;
  createBranch(name: string, startPoint: string): Promise<void>;
  switchBranch(name: string): Promise<void>;
  deleteBranch(name: string, force: boolean): Promise<void>;
  renameBranch(oldName: string, newName: string): Promise<void>;
  ```

No dedicated test: `RepoClient.ts` is a type-only interface plus a DTO shape, and
`tauriRepoClient.ts` is exercised indirectly by every component test that mocks `RepoClient`
(Task 6 onward) — matching how the existing 11 methods have no direct test of their own either.

- [ ] **Step 1: Add the type and interface methods**

In `frontend/src/ipc/RepoClient.ts`, add (after the `DiffHunk` interface, before
`export interface RepoClient`):

```ts
export interface BranchInfo {
  name: string;
  isCurrent: boolean;
}
```

Extend `RepoClient` (after `commit`):

```ts
  listBranches(): Promise<BranchInfo[]>;
  createBranch(name: string, startPoint: string): Promise<void>;
  switchBranch(name: string): Promise<void>;
  deleteBranch(name: string, force: boolean): Promise<void>;
  renameBranch(oldName: string, newName: string): Promise<void>;
```

- [ ] **Step 2: Implement in `tauriRepoClient.ts`**

In `frontend/src/ipc/tauriRepoClient.ts`, add `BranchInfo` to the type-only import and add the
five entries (after `commit`):

```ts
import type {
  BranchInfo,
  CommitInfo,
  DiffHunk,
  RepoClient,
  StatusEntry,
} from "./RepoClient";
```

```ts
  listBranches: () => invoke<BranchInfo[]>("list_branches"),
  createBranch: (name: string, startPoint: string) =>
    invoke("create_branch", { name, startPoint }),
  switchBranch: (name: string) => invoke("switch_branch", { name }),
  deleteBranch: (name: string, force: boolean) =>
    invoke("delete_branch", { name, force }),
  renameBranch: (oldName: string, newName: string) =>
    invoke("rename_branch", { oldName, newName }),
```

- [ ] **Step 3: Verify it builds and lints**

Run: `cd frontend && pnpm build && pnpm lint`
Expected: both succeed — this task only adds types/pass-through calls, so a clean build/lint is
the only signal needed. (Every other frontend consumer of `RepoClient` is still a plain object
literal implementing the old, narrower interface — that's expected to start failing to compile
until Task 6's mock objects are updated; if `pnpm build` fails only inside test files, that's
fine here, it's `pnpm test` that Task 6 fixes.)

- [ ] **Step 4: Commit**

```bash
git add frontend/src/ipc/RepoClient.ts frontend/src/ipc/tauriRepoClient.ts
git commit -m "feat(frontend): add branch methods to RepoClient"
```

---

### Task 6: `useAppState` — branch state + mutations

**Files:**
- Modify: `frontend/src/state/useAppState.ts`
- Modify: `frontend/src/state/useAppState.test.ts`

**Interfaces:**
- Consumes: `RepoClient.{listBranches, createBranch, switchBranch, deleteBranch, renameBranch}`
  from Task 5.
- Produces (used by Tasks 7–9):
  ```ts
  // on AppState:
  branches: BranchInfo[];
  createBranchDraft: { startPoint: string } | null;
  // on UseAppStateResult:
  createBranch(name: string, startPoint: string): Promise<void>;
  switchBranch(name: string): Promise<void>;
  deleteBranch(name: string, force: boolean): Promise<void>;
  renameBranch(oldName: string, newName: string): Promise<void>;
  openCreateBranchDraft(startPoint: string): void;
  closeCreateBranchDraft(): void;
  ```
  `createBranchDraft` is the one piece of UI-coordination state added here rather than kept
  local to a component: it lets `HistoryList`'s "Branch from here" context-menu action (Task 8)
  open `BranchSwitcher`'s create form (Task 7) pre-filled with a specific commit id, the same way
  `selectedRow` already coordinates cross-component selection state.

- [ ] **Step 1: Write the failing tests**

Add to `frontend/src/state/useAppState.test.ts`. Every existing fake `RepoClient` object literal
in this file must also gain the five new methods (`unimplemented()` is fine for those irrelevant
to the given test) — do this for all four existing `it()` blocks' `client` objects, then add:

```ts
  it("openRepo also populates branches", async () => {
    const branch: BranchInfo = { name: "main", isCurrent: true };
    const client: RepoClient = {
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      openRepo: async () => {},
      getStatus: async () => [],
      getLog: async () => [],
      listBranches: async () => [branch],
      createBranch: async () => unimplemented(),
      switchBranch: async () => unimplemented(),
      deleteBranch: async () => unimplemented(),
      renameBranch: async () => unimplemented(),
      getWorkingDiff: async () => unimplemented(),
      getCommitDiff: async () => unimplemented(),
      getCommitFiles: async () => unimplemented(),
      stageFile: async () => unimplemented(),
      unstageFile: async () => unimplemented(),
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client));

    await act(() => result.current.openRepo("/repo"));

    expect(result.current.state.branches).toEqual([branch]);
  });

  it("switchBranch calls client.switchBranch then refreshes branches", async () => {
    let switchArg: string | null = null;
    let branchesCallCount = 0;
    const client: RepoClient = {
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      openRepo: async () => {},
      getStatus: async () => [],
      getLog: async () => [],
      listBranches: async () => {
        branchesCallCount += 1;
        return branchesCallCount === 1
          ? [{ name: "main", isCurrent: true }]
          : [{ name: "feature", isCurrent: true }, { name: "main", isCurrent: false }];
      },
      createBranch: async () => unimplemented(),
      switchBranch: async (name: string) => {
        switchArg = name;
      },
      deleteBranch: async () => unimplemented(),
      renameBranch: async () => unimplemented(),
      getWorkingDiff: async () => unimplemented(),
      getCommitDiff: async () => unimplemented(),
      getCommitFiles: async () => unimplemented(),
      stageFile: async () => unimplemented(),
      unstageFile: async () => unimplemented(),
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));

    await act(() => result.current.switchBranch("feature"));

    expect(switchArg).toBe("feature");
    expect(result.current.state.branches).toEqual([
      { name: "feature", isCurrent: true },
      { name: "main", isCurrent: false },
    ]);
  });

  it("createBranch calls client.createBranch and clears the create-branch draft", async () => {
    let createArgs: [string, string] | null = null;
    const client: RepoClient = {
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      openRepo: async () => {},
      getStatus: async () => [],
      getLog: async () => [],
      listBranches: async () => [],
      createBranch: async (name: string, startPoint: string) => {
        createArgs = [name, startPoint];
      },
      switchBranch: async () => unimplemented(),
      deleteBranch: async () => unimplemented(),
      renameBranch: async () => unimplemented(),
      getWorkingDiff: async () => unimplemented(),
      getCommitDiff: async () => unimplemented(),
      getCommitFiles: async () => unimplemented(),
      stageFile: async () => unimplemented(),
      unstageFile: async () => unimplemented(),
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));
    act(() => result.current.openCreateBranchDraft("abc123"));
    expect(result.current.state.createBranchDraft).toEqual({ startPoint: "abc123" });

    await act(() => result.current.createBranch("feature", "abc123"));

    expect(createArgs).toEqual(["feature", "abc123"]);
    expect(result.current.state.createBranchDraft).toBeNull();
  });

  it("closeCreateBranchDraft clears the draft without calling the client", async () => {
    const client: RepoClient = {
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      openRepo: async () => {},
      getStatus: async () => [],
      getLog: async () => [],
      listBranches: async () => [],
      createBranch: async () => unimplemented(),
      switchBranch: async () => unimplemented(),
      deleteBranch: async () => unimplemented(),
      renameBranch: async () => unimplemented(),
      getWorkingDiff: async () => unimplemented(),
      getCommitDiff: async () => unimplemented(),
      getCommitFiles: async () => unimplemented(),
      stageFile: async () => unimplemented(),
      unstageFile: async () => unimplemented(),
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client));
    act(() => result.current.openCreateBranchDraft("HEAD"));

    act(() => result.current.closeCreateBranchDraft());

    expect(result.current.state.createBranchDraft).toBeNull();
  });
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm test -- --run useAppState`
Expected: FAIL to compile — `branches`, `createBranchDraft`, and the four new methods don't
exist on `AppState`/`UseAppStateResult` yet, and the fake `client` objects are missing the new
`RepoClient` methods.

- [ ] **Step 3: Write the implementation**

In `frontend/src/state/useAppState.ts`:

```ts
import { useCallback, useState } from "react";
import type { BranchInfo, CommitInfo, RepoClient, StatusEntry } from "../ipc/RepoClient";

const LOG_LIMIT = 300;

export type SelectedRow = "uncommitted" | { commitId: string };

export interface AppState {
  repoPath: string | null;
  selectedRow: SelectedRow;
  status: StatusEntry[];
  log: CommitInfo[];
  branches: BranchInfo[];
  createBranchDraft: { startPoint: string } | null;
  error: string | null;
}

export interface UseAppStateResult {
  state: AppState;
  openRepo(path: string): Promise<void>;
  selectRow(row: SelectedRow): void;
  stageFile(path: string): Promise<void>;
  unstageFile(path: string): Promise<void>;
  commit(message: string): Promise<void>;
  createBranch(name: string, startPoint: string): Promise<void>;
  switchBranch(name: string): Promise<void>;
  deleteBranch(name: string, force: boolean): Promise<void>;
  renameBranch(oldName: string, newName: string): Promise<void>;
  openCreateBranchDraft(startPoint: string): void;
  closeCreateBranchDraft(): void;
  refresh(): Promise<void>;
}

export function useAppState(client: RepoClient): UseAppStateResult {
  const [state, setState] = useState<AppState>({
    repoPath: null,
    selectedRow: "uncommitted",
    status: [],
    log: [],
    branches: [],
    createBranchDraft: null,
    error: null,
  });

  const refresh = useCallback(async () => {
    try {
      const [status, log, branches] = await Promise.all([
        client.getStatus(),
        client.getLog(LOG_LIMIT),
        client.listBranches(),
      ]);
      setState((prev) => ({ ...prev, status, log, branches, error: null }));
    } catch (err) {
      setState((prev) => ({ ...prev, error: String(err) }));
    }
  }, [client]);

  const runMutation = useCallback(
    async (mutate: () => Promise<void>) => {
      try {
        await mutate();
        await refresh();
      } catch (err) {
        setState((prev) => ({ ...prev, error: String(err) }));
      }
    },
    [refresh],
  );

  const openRepo = useCallback(
    (path: string) =>
      runMutation(async () => {
        await client.openRepo(path);
        setState((prev) => ({ ...prev, repoPath: path, selectedRow: "uncommitted" }));
      }),
    [client, runMutation],
  );

  const selectRow = useCallback((row: SelectedRow) => {
    setState((prev) => ({ ...prev, selectedRow: row }));
  }, []);

  const stageFile = useCallback(
    (path: string) => runMutation(() => client.stageFile(path)),
    [client, runMutation],
  );
  const unstageFile = useCallback(
    (path: string) => runMutation(() => client.unstageFile(path)),
    [client, runMutation],
  );
  const commit = useCallback(
    (message: string) => runMutation(() => client.commit(message)),
    [client, runMutation],
  );

  const createBranch = useCallback(
    (name: string, startPoint: string) =>
      runMutation(async () => {
        await client.createBranch(name, startPoint);
        setState((prev) => ({ ...prev, createBranchDraft: null }));
      }),
    [client, runMutation],
  );
  const switchBranch = useCallback(
    (name: string) => runMutation(() => client.switchBranch(name)),
    [client, runMutation],
  );
  const deleteBranch = useCallback(
    (name: string, force: boolean) => runMutation(() => client.deleteBranch(name, force)),
    [client, runMutation],
  );
  const renameBranch = useCallback(
    (oldName: string, newName: string) => runMutation(() => client.renameBranch(oldName, newName)),
    [client, runMutation],
  );

  const openCreateBranchDraft = useCallback((startPoint: string) => {
    setState((prev) => ({ ...prev, createBranchDraft: { startPoint } }));
  }, []);
  const closeCreateBranchDraft = useCallback(() => {
    setState((prev) => ({ ...prev, createBranchDraft: null }));
  }, []);

  return {
    state,
    openRepo,
    selectRow,
    stageFile,
    unstageFile,
    commit,
    createBranch,
    switchBranch,
    deleteBranch,
    renameBranch,
    openCreateBranchDraft,
    closeCreateBranchDraft,
    refresh,
  };
}
```

In each of the four pre-existing fake `client` object literals already in `useAppState.test.ts`
(inside `"openRepo populates status and log and sets repoPath"`, `"selectRow updates selectedRow
without refetching"`, `"stageFile calls client.stageFile then refreshes status"`, and `"errors
surface in state.error without throwing"`), insert this exact block immediately after the
object's `getLog: async () => ...,` line and before its `getWorkingDiff: async () =>
unimplemented(),` line:

```ts
      listBranches: async () => [],
      createBranch: async () => unimplemented(),
      switchBranch: async () => unimplemented(),
      deleteBranch: async () => unimplemented(),
      renameBranch: async () => unimplemented(),
```

(`listBranches` returns `[]` rather than `unimplemented()` because `refresh()` now always calls
it, including in these four tests, none of which care about its result.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && pnpm test -- --run useAppState`
Expected: PASS (8 tests: 4 existing + 4 new).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/state/useAppState.ts frontend/src/state/useAppState.test.ts
git commit -m "feat(frontend): add branch state and mutations to useAppState"
```

---

### Task 7: `BranchSwitcher` component

**Files:**
- Create: `frontend/src/components/BranchSwitcher.tsx`
- Create: `frontend/src/components/BranchSwitcher.test.tsx`

**Interfaces:**
- Consumes: `BranchInfo` from `../ipc/RepoClient`; the six `useAppState` branch-related pieces
  from Task 6 (`branches`, `createBranchDraft`, `createBranch`, `switchBranch`, `deleteBranch`,
  `renameBranch`, `openCreateBranchDraft`, `closeCreateBranchDraft`), passed in as props (this
  component never imports `useAppState` or `RepoClient` directly — matching `HistoryList`'s and
  `DiffPane`'s existing prop-driven style).
- Produces (used by Task 9): the `BranchSwitcher` component, plus a stable
  `aria-label="Branch switcher"` on its toggle button, used by both a future test and the E2E
  spec (Task 10) as a selector that doesn't depend on the fixture repo's default branch name.

- [ ] **Step 1: Write the failing tests**

Create `frontend/src/components/BranchSwitcher.test.tsx`:

```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { BranchInfo } from "../ipc/RepoClient";
import { BranchSwitcher } from "./BranchSwitcher";

const branches: BranchInfo[] = [
  { name: "main", isCurrent: true },
  { name: "feature", isCurrent: false },
];

function renderSwitcher(overrides: Partial<Parameters<typeof BranchSwitcher>[0]> = {}) {
  return render(
    <BranchSwitcher
      branches={branches}
      createBranchDraft={null}
      onSwitchBranch={vi.fn()}
      onCreateBranch={vi.fn()}
      onDeleteBranch={vi.fn()}
      onRenameBranch={vi.fn()}
      onOpenCreateBranchDraft={vi.fn()}
      onCloseCreateBranchDraft={vi.fn()}
      {...overrides}
    />,
  );
}

describe("BranchSwitcher", () => {
  it("shows the current branch name on the toggle button", () => {
    renderSwitcher();

    expect(screen.getByRole("button", { name: "Branch switcher" })).toHaveTextContent("main");
  });

  it("opening the switcher lists all branches, clicking one switches", () => {
    const onSwitchBranch = vi.fn();
    renderSwitcher({ onSwitchBranch });

    fireEvent.click(screen.getByRole("button", { name: "Branch switcher" }));
    fireEvent.click(screen.getByText("feature"));

    expect(onSwitchBranch).toHaveBeenCalledWith("feature");
  });

  it("New Branch… opens the create-branch draft with startPoint HEAD", () => {
    const onOpenCreateBranchDraft = vi.fn();
    renderSwitcher({ onOpenCreateBranchDraft });

    fireEvent.click(screen.getByRole("button", { name: "Branch switcher" }));
    fireEvent.click(screen.getByText("New Branch…"));

    expect(onOpenCreateBranchDraft).toHaveBeenCalledWith("HEAD");
  });

  it("a non-null createBranchDraft shows the create form; submitting calls onCreateBranch with its startPoint", () => {
    const onCreateBranch = vi.fn();
    renderSwitcher({ createBranchDraft: { startPoint: "abc123" }, onCreateBranch });

    fireEvent.change(screen.getByPlaceholderText("New branch name"), {
      target: { value: "my-feature" },
    });
    fireEvent.click(screen.getByText("Create"));

    expect(onCreateBranch).toHaveBeenCalledWith("my-feature", "abc123");
  });

  it("Cancel in the create form calls onCloseCreateBranchDraft", () => {
    const onCloseCreateBranchDraft = vi.fn();
    renderSwitcher({ createBranchDraft: { startPoint: "HEAD" }, onCloseCreateBranchDraft });

    fireEvent.click(screen.getByText("Cancel"));

    expect(onCloseCreateBranchDraft).toHaveBeenCalled();
  });

  it("clicking Delete once calls onDeleteBranch with force=false; a second click (still listed) forces it", async () => {
    const onDeleteBranch = vi.fn().mockResolvedValue(undefined);
    renderSwitcher({ onDeleteBranch });

    fireEvent.click(screen.getByRole("button", { name: "Branch switcher" }));
    const deleteButtons = screen.getAllByText("Delete");
    fireEvent.click(deleteButtons[1]); // "feature" row — index 1 in the branches fixture above
    await Promise.resolve();

    expect(onDeleteBranch).toHaveBeenCalledWith("feature", false);

    // Since `branches` prop is unchanged (delete didn't actually remove it, as this fixture's
    // parent never updates the prop), the row now shows "Force Delete" instead of "Delete".
    fireEvent.click(await screen.findByText("Force Delete"));

    expect(onDeleteBranch).toHaveBeenCalledWith("feature", true);
  });

  it("Rename shows an inline input; Enter calls onRenameBranch", () => {
    const onRenameBranch = vi.fn();
    renderSwitcher({ onRenameBranch });

    fireEvent.click(screen.getByRole("button", { name: "Branch switcher" }));
    const renameButtons = screen.getAllByText("Rename");
    fireEvent.click(renameButtons[1]); // "feature" row
    const input = screen.getByDisplayValue("feature");
    fireEvent.change(input, { target: { value: "feature-renamed" } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(onRenameBranch).toHaveBeenCalledWith("feature", "feature-renamed");
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm test -- --run BranchSwitcher`
Expected: FAIL — `./BranchSwitcher` doesn't exist yet.

- [ ] **Step 3: Write the implementation**

Create `frontend/src/components/BranchSwitcher.tsx`:

```tsx
import { useState, type KeyboardEvent } from "react";
import type { BranchInfo } from "../ipc/RepoClient";

export function BranchSwitcher({
  branches,
  createBranchDraft,
  onSwitchBranch,
  onCreateBranch,
  onDeleteBranch,
  onRenameBranch,
  onOpenCreateBranchDraft,
  onCloseCreateBranchDraft,
}: {
  branches: BranchInfo[];
  createBranchDraft: { startPoint: string } | null;
  onSwitchBranch: (name: string) => void;
  onCreateBranch: (name: string, startPoint: string) => void;
  onDeleteBranch: (name: string, force: boolean) => Promise<void>;
  onRenameBranch: (oldName: string, newName: string) => void;
  onOpenCreateBranchDraft: (startPoint: string) => void;
  onCloseCreateBranchDraft: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [newBranchName, setNewBranchName] = useState("");
  const [pendingForceFor, setPendingForceFor] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");

  const current = branches.find((b) => b.isCurrent);

  const submitCreate = () => {
    if (newBranchName.trim() === "" || createBranchDraft === null) {
      return;
    }
    onCreateBranch(newBranchName.trim(), createBranchDraft.startPoint);
    setNewBranchName("");
  };

  const handleDeleteClick = async (name: string) => {
    await onDeleteBranch(name, false);
    // useAppState swallows a rejected mutation into state.error rather than rethrowing, so the
    // only reliable "did it actually delete" signal here is whether `name` is still present in
    // `branches` on the next render — a successful delete drops it from the list entirely,
    // which also makes the "Force Delete" button below disappear along with the row.
    setPendingForceFor(name);
  };

  const handleRenameKeyDown = (event: KeyboardEvent<HTMLInputElement>, oldName: string) => {
    if (event.key === "Enter") {
      onRenameBranch(oldName, renameValue);
      setRenaming(null);
    }
  };

  return (
    <div>
      <button aria-label="Branch switcher" onClick={() => setOpen((o) => !o)}>
        {current?.name ?? "no branch"}
      </button>
      {open && (
        <div>
          <ul>
            {branches.map((b) => (
              <li key={b.name}>
                {renaming === b.name ? (
                  <input
                    value={renameValue}
                    onChange={(event) => setRenameValue(event.target.value)}
                    onKeyDown={(event) => handleRenameKeyDown(event, b.name)}
                  />
                ) : (
                  <button
                    onClick={() => {
                      onSwitchBranch(b.name);
                      setOpen(false);
                    }}
                  >
                    {b.name}
                    {b.isCurrent && " (current)"}
                  </button>
                )}
                <button
                  onClick={() => {
                    setRenaming(b.name);
                    setRenameValue(b.name);
                  }}
                >
                  Rename
                </button>
                {pendingForceFor === b.name ? (
                  <button
                    onClick={() => {
                      onDeleteBranch(b.name, true);
                      setPendingForceFor(null);
                    }}
                  >
                    Force Delete
                  </button>
                ) : (
                  <button onClick={() => handleDeleteClick(b.name)}>Delete</button>
                )}
              </li>
            ))}
          </ul>
          <button onClick={() => onOpenCreateBranchDraft("HEAD")}>New Branch…</button>
        </div>
      )}
      {createBranchDraft !== null && (
        <div>
          <input
            value={newBranchName}
            onChange={(event) => setNewBranchName(event.target.value)}
            placeholder="New branch name"
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                submitCreate();
              }
            }}
          />
          <button onClick={submitCreate} disabled={newBranchName.trim() === ""}>
            Create
          </button>
          <button onClick={onCloseCreateBranchDraft}>Cancel</button>
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && pnpm test -- --run BranchSwitcher`
Expected: PASS (7 tests).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/BranchSwitcher.tsx frontend/src/components/BranchSwitcher.test.tsx
git commit -m "feat(frontend): add BranchSwitcher component"
```

---

### Task 8: `HistoryList` — "Branch from here" context menu

**Files:**
- Modify: `frontend/src/components/HistoryList.tsx`
- Modify: `frontend/src/components/HistoryList.test.tsx`

**Interfaces:**
- Consumes: nothing new from other tasks directly — adds one new prop.
- Produces (used by Task 9):
  ```ts
  // new HistoryList prop:
  onBranchFromCommit: (commitId: string) => void;
  ```

- [ ] **Step 1: Write the failing tests**

Add to `frontend/src/components/HistoryList.test.tsx` (every existing `render(<HistoryList ...
/>)` call in this file must also gain `onBranchFromCommit={vi.fn()}` — add that prop to all six
existing calls), then append:

```tsx
  it("right-clicking a commit row shows a 'Branch from here' menu entry", () => {
    render(
      <HistoryList
        status={status}
        log={log}
        selectedRow="uncommitted"
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
      />,
    );

    fireEvent.contextMenu(screen.getByText(/second commit/).closest("li")!);

    expect(screen.getByText("Branch from here")).toBeInTheDocument();
  });

  it("clicking 'Branch from here' calls onBranchFromCommit with that commit's id and closes the menu", () => {
    const onBranchFromCommit = vi.fn();
    render(
      <HistoryList
        status={status}
        log={log}
        selectedRow="uncommitted"
        onSelectRow={vi.fn()}
        onBranchFromCommit={onBranchFromCommit}
      />,
    );

    fireEvent.contextMenu(screen.getByText(/second commit/).closest("li")!);
    fireEvent.click(screen.getByText("Branch from here"));

    expect(onBranchFromCommit).toHaveBeenCalledWith("aaa111...");
    expect(screen.queryByText("Branch from here")).not.toBeInTheDocument();
  });

  it("right-clicking the Uncommitted Changes row does not show the menu", () => {
    render(
      <HistoryList
        status={status}
        log={log}
        selectedRow="uncommitted"
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
      />,
    );

    fireEvent.contextMenu(screen.getByText(/Uncommitted Changes/).closest("li")!);

    expect(screen.queryByText("Branch from here")).not.toBeInTheDocument();
  });
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm test -- --run HistoryList`
Expected: FAIL — `onBranchFromCommit` is a required prop the existing calls don't pass yet, and
the context-menu behavior doesn't exist.

- [ ] **Step 3: Write the implementation**

Replace the full contents of `frontend/src/components/HistoryList.tsx`:

```tsx
import { useState, type KeyboardEvent, type MouseEvent } from "react";
import type { CommitInfo, StatusEntry } from "../ipc/RepoClient";
import type { SelectedRow } from "../state/useAppState";

function rowsEqual(a: SelectedRow, b: SelectedRow): boolean {
  if (a === "uncommitted" || b === "uncommitted") {
    return a === b;
  }
  return a.commitId === b.commitId;
}

export function HistoryList({
  status,
  log,
  selectedRow,
  onSelectRow,
  onBranchFromCommit,
}: {
  status: StatusEntry[];
  log: CommitInfo[];
  selectedRow: SelectedRow;
  onSelectRow: (row: SelectedRow) => void;
  onBranchFromCommit: (commitId: string) => void;
}) {
  const [contextMenu, setContextMenu] = useState<{
    commitId: string;
    x: number;
    y: number;
  } | null>(null);

  const rows: SelectedRow[] = [
    "uncommitted",
    ...log.map((commit) => ({ commitId: commit.id })),
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
      {log.map((commit) => (
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

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && pnpm test -- --run HistoryList`
Expected: PASS (9 tests: 6 existing + 3 new).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/HistoryList.tsx frontend/src/components/HistoryList.test.tsx
git commit -m "feat(frontend): add 'Branch from here' context menu to HistoryList"
```

---

### Task 9: Wire `BranchSwitcher` and `HistoryList` into `App.tsx`

**Files:**
- Modify: `frontend/src/App.tsx`

**Interfaces:**
- Consumes: `BranchSwitcher` (Task 7), `HistoryList`'s new `onBranchFromCommit` prop (Task 8),
  and `useAppState`'s branch fields/methods (Task 6).
- Produces: nothing further downstream — this is the final integration point for this feature.

No dedicated unit test: `App.tsx` has no existing test file (it's covered by the E2E layer, per
`docs/ARCHITECTURE.md`'s testing strategy — component-level logic is unit-tested, cross-component
wiring is E2E-tested). Task 10 adds that E2E coverage.

- [ ] **Step 1: Wire it up**

Replace the full contents of `frontend/src/App.tsx`:

```tsx
import { useEffect } from "react";
import { BranchSwitcher } from "./components/BranchSwitcher";
import { DiffPane } from "./components/DiffPane";
import { HistoryList } from "./components/HistoryList";
import { RepoPicker } from "./components/RepoPicker";
import { tauriRepoClient } from "./ipc/tauriRepoClient";
import { useAppState } from "./state/useAppState";

export default function App() {
  const appState = useAppState(tauriRepoClient);

  // E2E-only auto-open: `RepoPicker`'s native folder dialog can't be driven through WebDriver,
  // so the E2E build points at a fixture repo via this Vite env var instead. Statically absent
  // from a normal production build unless VITE_E2E_REPO_PATH is set at build time.
  useEffect(() => {
    const autoOpenPath = import.meta.env.VITE_E2E_REPO_PATH;
    if (typeof autoOpenPath === "string" && autoOpenPath.length > 0) {
      appState.openRepo(autoOpenPath);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (appState.state.repoPath === null) {
    return (
      <main>
        <h1>Browsitory</h1>
        {/* `RepoPicker` only surfaces errors from its own `pickRepoFolder`/`listRecentRepos`
            calls; an `onOpenRepo` rejection (bad path, a stale recent-repo entry, permissions)
            lands in `useAppState`'s `state.error`, which is otherwise only rendered in the
            post-open branch below — leaving a failed open looking like nothing happened. */}
        {appState.state.error !== null && <p role="alert">{appState.state.error}</p>}
        <RepoPicker client={tauriRepoClient} onOpenRepo={appState.openRepo} />
      </main>
    );
  }

  return (
    <main>
      <h1>Browsitory</h1>
      {appState.state.error !== null && <p role="alert">{appState.state.error}</p>}
      <BranchSwitcher
        branches={appState.state.branches}
        createBranchDraft={appState.state.createBranchDraft}
        onSwitchBranch={appState.switchBranch}
        onCreateBranch={appState.createBranch}
        onDeleteBranch={appState.deleteBranch}
        onRenameBranch={appState.renameBranch}
        onOpenCreateBranchDraft={appState.openCreateBranchDraft}
        onCloseCreateBranchDraft={appState.closeCreateBranchDraft}
      />
      <div className="app-layout">
        <HistoryList
          status={appState.state.status}
          log={appState.state.log}
          selectedRow={appState.state.selectedRow}
          onSelectRow={appState.selectRow}
          onBranchFromCommit={appState.openCreateBranchDraft}
        />
        <DiffPane
          client={tauriRepoClient}
          selectedRow={appState.state.selectedRow}
          status={appState.state.status}
          onStageFile={appState.stageFile}
          onUnstageFile={appState.unstageFile}
          onCommit={appState.commit}
        />
      </div>
    </main>
  );
}
```

(`onBranchFromCommit={appState.openCreateBranchDraft}` works directly — both have the signature
`(startPoint: string) => void` with `startPoint` being the commit id in this call path — no
adapter function needed.)

- [ ] **Step 2: Run the full frontend test suite**

Run: `cd frontend && pnpm test -- --run && pnpm lint && pnpm build`
Expected: all PASS — this step catches any prop-shape mismatch across the whole wiring.

- [ ] **Step 3: Manually verify in the running app**

Run: `cargo tauri dev`
Expected: the branch switcher button appears above the history/diff layout, showing the current
branch; clicking it lists branches and offers "New Branch…"; right-clicking a commit row shows
"Branch from here" and opens the create form pre-filled with that commit's id.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/App.tsx
git commit -m "feat(frontend): wire BranchSwitcher into App"
```

---

### Task 10: E2E flow — create and switch branches

**Files:**
- Create: `e2e/specs/branch-management.spec.ts`

**Interfaces:**
- Consumes: the built app from Tasks 1–9, driven as a black box via `tauri-driver` + WebdriverIO
  (same harness as `e2e/specs/first-flow.spec.ts`).
- Produces: nothing downstream — this is the last task in the plan.

- [ ] **Step 1: Write the E2E spec**

Create `e2e/specs/branch-management.spec.ts`:

```ts
import { expect } from "@wdio/globals";

describe("Browsitory branch management", () => {
  it("creates a branch from HEAD, switches to it, and shows it as current", async () => {
    const switcherButton = await $('[aria-label="Branch switcher"]');
    await switcherButton.waitForExist({ timeout: 10000 });
    await switcherButton.click();

    const newBranchButton = await $("button=New Branch…");
    await newBranchButton.click();

    const nameInput = await $("input[placeholder='New branch name']");
    await nameInput.setValue("feature/e2e-branch");
    const createButton = await $("button=Create");
    await createButton.click();

    // Switching happens automatically on create; the toggle button's label should now read
    // the new branch's name.
    await browser.waitUntil(
      async () => (await switcherButton.getText()) === "feature/e2e-branch",
      { timeout: 10000, timeoutMsg: "expected the switcher to show the new branch as current" },
    );
  });
});
```

- [ ] **Step 2: Build and run**

Run (from repo root, per `CLAUDE.md`'s E2E sequence):
```bash
cd frontend && VITE_E2E_REPO_PATH=/tmp/browsitory-e2e-repo pnpm build && cd ..
cargo build --workspace --features tauri-app/custom-protocol
cd e2e && pnpm install && pnpm test
```
Expected: both `first-flow.spec.ts` and the new `branch-management.spec.ts` PASS.

- [ ] **Step 3: Commit**

```bash
git add e2e/specs/branch-management.spec.ts
git commit -m "test(e2e): add branch create/switch flow"
```

---

## Self-Review Notes

- **Spec coverage:** list/create/switch/delete/rename all implemented (Tasks 1–2 backend,
  Tasks 5–8 frontend); dirty-switch blocked via git2 safe checkout, no frontend pre-check (Task
  1); delete blocked on unmerged commits with explicit force retry (Task 2 backend, Task 7
  frontend UX); create-from-HEAD-or-selected-commit via `startPoint` + `createBranchDraft` (Tasks
  1, 6, 8); auto-switch on create (Task 1's `create_branch` calls `switch_branch`); branch state
  folded into `useAppState`, not a separate store (Task 6); one E2E flow (Task 10). All covered.
- **Placeholder scan:** none found — every step has real code, real test bodies, real commands.
- **Type consistency:** `BranchInfo { name, isCurrent }` used identically in Tasks 5–9;
  `createBranchDraft: { startPoint: string } | null` used identically in Tasks 6–9;
  `onDeleteBranch`/`deleteBranch` signature `(name: string, force: boolean) => Promise<void>`
  matches from Task 6 through Task 7 exactly (Task 7's local type spells the return type as
  `Promise<void>` explicitly, since `BranchSwitcher.tsx` awaits it — checked against Task 6's
  `useAppState.deleteBranch`, whose `runMutation`-wrapped return type is also `Promise<void>`).
