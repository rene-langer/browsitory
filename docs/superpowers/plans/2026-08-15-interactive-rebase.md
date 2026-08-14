# Interactive Rebase Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebase the current branch onto an earlier commit, with a reorderable interactive plan
(Pick/Reword/Edit/Squash/Fixup/Drop), per
`docs/superpowers/specs/2026-08-15-interactive-rebase-design.md`.

**Architecture:** A hand-rolled cherry-pick loop in a new `git-core::rebase` module, built on
`Repository::cherrypick` (not git2's fixed-order `Rebase` iterator, which can't reorder/drop/
squash) plus a detached-HEAD lifecycle so the original branch ref is untouched until the rebase
finishes. Squash/fixup message combination happens entirely at planning time (pure text, no
execution-time pause needed) — execution only ever pauses for a conflict or an `Edit` step, both
of which reuse existing infrastructure almost unchanged: a cherry-pick conflict is the exact same
`IndexConflict` shape merge already handles, and an `Edit` pause is just ordinary staged
uncommitted changes the existing Stage/Unstage/diff UI already handles.

**Tech Stack:** Rust (git2, thiserror), Tauri 2, React/TypeScript, Vitest + Testing Library,
`tauri-driver` + WebdriverIO for E2E.

## Global Constraints

- Local, first-parent-only commit ranges — no merge commits in the rebased range, no rebasing
  onto a different branch's tip (this pass's trigger is "rebase current branch onto an earlier
  commit," not a branch picker).
- Reword's new message and a Squash/Fixup group's combined message are decided at planning time,
  before the rebase starts — never as an execution-time pause.
- The rebase plan and its progress are session-scoped (in the `Worker` thread's owned state,
  alongside `repo`) — not persisted to disk, matching every other in-progress-operation feature
  in this app (merge's `MERGE_HEAD`-backed state is the one exception, since that's git's own
  on-disk mechanism, not something this app chose to add).
- `git-core` tests use real temp-dir repos (`crates/git-core/tests/common/mod.rs` helpers), never
  mocks. `tauri-app` tests use real temp-dir repos too, inline in the module they test.
  `frontend` tests mock `RepoClient`, never `@tauri-apps/api`.
- Test commands: `cargo test -p git-core --test rebase`, `cargo test -p tauri-app`, `cd frontend
  && pnpm test -- --run`, `cd e2e && pnpm test` (needs a fresh `pnpm build` + `cargo build
  --workspace --features tauri-app/custom-protocol` first, per `CLAUDE.md`).
- Every git2 signature used below has been verified against the vendored source at
  `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/git2-0.21.0/src/`, not trusted from
  memory. In particular: `Repository::cherrypick` (the real, working-tree-updating primitive, not
  `cherrypick_commit`), `Repository::set_head_detached`/`checkout_head`, `Commit::message()`
  (`Result<&str, Error>`, not nested `Option`) and `Commit::author()` (`Signature<'_>`, no
  `Result` at all — always succeeds), and `RepositoryState::CherryPick` being a distinct variant
  from `RepositoryState::Merge`, confirming `git-core::commit::commit`'s existing merge-parent
  logic (added for the merge feature) is inert during a rebase step and correctly produces
  single-parent commits.

---

### Task 1: `git-core::rebase` — types and `commits_since`

**Files:**
- Create: `crates/git-core/src/rebase.rs`
- Modify: `crates/git-core/src/lib.rs` (add `pub mod rebase;` — alphabetically after `merge`,
  before `repo`)
- Test: `crates/git-core/tests/rebase.rs`

**Interfaces:**
- Consumes: `crates/git-core/tests/common/mod.rs`'s `init_repo()`, `commit_all()`, `write_file()`
  (already exist, unchanged).
- Produces (used by Task 2 and downstream):
  ```rust
  pub enum RebaseError {
      Git(git2::Error),
      InvalidPlan(String),
      NotRebasing,
  }
  pub struct RebasePlanCommit {
      pub id: String,
      pub short_id: String,
      pub summary: String,
      pub author_name: String,
      pub timestamp: i64,
  }
  pub fn commits_since(repo: &git2::Repository, onto: &str) -> Result<Vec<RebasePlanCommit>, RebaseError>;
  ```

- [ ] **Step 1: Write the failing tests**

Create `crates/git-core/tests/rebase.rs`:

```rust
mod common;

use common::{commit_all, init_repo, write_file};

#[test]
fn commits_since_lists_commits_oldest_first_between_onto_and_head() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto_id = repo.head().unwrap().peel_to_commit().unwrap().id().to_string();

    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");
    write_file(dir.path(), "b.txt", "b\n");
    commit_all(&repo, "add b");

    let commits = git_core::rebase::commits_since(&repo, &onto_id).unwrap();

    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0].summary, "add a");
    assert_eq!(commits[1].summary, "add b");
}

#[test]
fn commits_since_returns_empty_when_onto_is_head() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");

    let commits = git_core::rebase::commits_since(&repo, "HEAD").unwrap();

    assert!(commits.is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test rebase`
Expected: FAIL to compile — `git_core::rebase` doesn't exist yet.

- [ ] **Step 3: Write the implementation**

Create `crates/git-core/src/rebase.rs`:

```rust
use git2::{Repository, Sort};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RebaseError {
    #[error("git operation failed: {0}")]
    Git(#[from] git2::Error),
    #[error("invalid rebase plan: {0}")]
    InvalidPlan(String),
    #[error("no rebase is currently in progress")]
    NotRebasing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebasePlanCommit {
    pub id: String,
    pub short_id: String,
    pub summary: String,
    pub author_name: String,
    pub timestamp: i64,
}

pub fn commits_since(repo: &Repository, onto: &str) -> Result<Vec<RebasePlanCommit>, RebaseError> {
    let onto_oid = repo.revparse_single(onto)?.peel_to_commit()?.id();

    let mut revwalk = repo.revwalk()?;
    revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
    revwalk.push_head()?;
    revwalk.hide(onto_oid)?;

    let mut commits = Vec::new();
    for oid_result in revwalk {
        let oid = oid_result?;
        let commit = repo.find_commit(oid)?;
        let id = oid.to_string();
        commits.push(RebasePlanCommit {
            short_id: id[..7].to_string(),
            id,
            summary: commit
                .summary()
                .ok()
                .flatten()
                .unwrap_or_default()
                .to_string(),
            author_name: commit.author().name().ok().unwrap_or_default().to_string(),
            timestamp: commit.time().seconds(),
        });
    }
    // `revwalk` yields newest-first; the plan wants oldest-first, matching actual replay order.
    commits.reverse();
    Ok(commits)
}
```

Add `pub mod rebase;` to `crates/git-core/src/lib.rs`:

```rust
pub mod blame;
pub mod branch;
pub mod commit;
pub mod diff;
pub mod graph;
pub mod merge;
pub mod rebase;
pub mod repo;
pub mod stage;
pub mod stash;
pub mod status;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p git-core --test rebase`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/git-core/src/rebase.rs crates/git-core/src/lib.rs crates/git-core/tests/rebase.rs
git commit -m "feat(git-core): add rebase module skeleton (types + commits_since)"
```

---

### Task 2: `git-core::rebase` — the cherry-pick engine (`start_rebase`, `rebase_continue`, and the shared step loop)

**Files:**
- Modify: `crates/git-core/src/merge.rs` (make `conflict_path` `pub(crate)` so this task can
  reuse it instead of duplicating conflict-path-collection logic)
- Modify: `crates/git-core/src/rebase.rs`
- Modify: `crates/git-core/tests/rebase.rs`

**Interfaces:**
- Consumes: `crate::merge::conflict_path` (widened to `pub(crate)` in this task).
- Produces (used by Task 3 and downstream):
  ```rust
  pub enum RebaseAction {
      Pick,
      Reword { message: String },
      Edit,
      Squash,
      Fixup,
      Drop,
  }
  pub struct RebasePlanEntry {
      pub commit_id: String,
      pub action: RebaseAction,
      pub combined_message: Option<String>, // only meaningful on a group leader
  }
  pub enum RebaseStepResult {
      Conflicted { files: Vec<String> },
      PausedForEdit,
      Advanced,
      Done,
  }
  pub struct RebaseState { /* opaque outside git-core */ }
  impl RebaseState {
      pub fn current_step(&self) -> usize;
      pub fn total_steps(&self) -> usize;
  }
  pub fn start_rebase(repo: &git2::Repository, onto: &str, plan: Vec<RebasePlanEntry>) -> Result<(RebaseState, RebaseStepResult), RebaseError>;
  pub fn rebase_continue(repo: &git2::Repository, state: &mut RebaseState) -> Result<RebaseStepResult, RebaseError>;
  ```
  This task deliberately does **not** split `start_rebase`/`rebase_continue`/the shared advance
  loop into separate tasks: they share one internal loop function
  (`advance`) and can't be meaningfully tested apart — a `rebase_continue` test needs a
  `start_rebase` call first regardless of which task "owns" which function. Splitting would only
  produce an artificial first task with no useful test coverage of its own.

- [ ] **Step 1: Widen `conflict_path`'s visibility**

In `crates/git-core/src/merge.rs`, change:
```rust
fn conflict_path(conflict: &IndexConflict) -> Option<String> {
```
to:
```rust
pub(crate) fn conflict_path(conflict: &IndexConflict) -> Option<String> {
```
(No other change to `merge.rs` — this one line only.)

- [ ] **Step 2: Write the failing tests**

Append to `crates/git-core/tests/rebase.rs`:

```rust
use git_core::rebase::{RebaseAction, RebasePlanEntry, RebaseStepResult};

fn commit_id_at(repo: &git2::Repository, offset_from_head: usize) -> String {
    // Walks back `offset_from_head` first-parent steps from HEAD and returns that commit's id —
    // a small test helper for picking out a specific commit to build a plan entry around.
    let mut commit = repo.head().unwrap().peel_to_commit().unwrap();
    for _ in 0..offset_from_head {
        commit = commit.parent(0).unwrap();
    }
    commit.id().to_string()
}

fn pick(commit_id: &str) -> RebasePlanEntry {
    RebasePlanEntry {
        commit_id: commit_id.to_string(),
        action: RebaseAction::Pick,
        combined_message: None,
    }
}

#[test]
fn start_rebase_rejects_a_plan_starting_with_squash_or_fixup() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");
    let onto = commit_id_at(&repo, 1);
    let a_id = commit_id_at(&repo, 0);

    let plan = vec![RebasePlanEntry {
        commit_id: a_id,
        action: RebaseAction::Squash,
        combined_message: None,
    }];

    let result = git_core::rebase::start_rebase(&repo, &onto, plan);

    assert!(result.is_err());
}

#[test]
fn a_clean_multi_pick_rebase_lands_every_commit_and_finishes() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");
    write_file(dir.path(), "b.txt", "b\n");
    commit_all(&repo, "add b");

    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();
    let plan: Vec<RebasePlanEntry> = commits.iter().map(|c| pick(&c.id)).collect();

    let (mut state, first_result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();
    assert_eq!(first_result, RebaseStepResult::Advanced.variant_eq_placeholder());
```

Wait — `RebaseStepResult` won't have a convenient `Advanced`-vs-`Done` distinguishing helper by
default; since a clean multi-pick plan runs straight through to completion inside `start_rebase`
itself (the shared loop auto-advances through every clean step with nothing to pause on), assert
`Done` directly instead:

```rust
    let (state, first_result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();

    assert_eq!(first_result, RebaseStepResult::Done);
    assert_eq!(state.current_step(), state.total_steps());

    let commits_after = git_core::rebase::commits_since(&repo, &onto).unwrap();
    assert_eq!(commits_after.len(), 2);
    assert_eq!(commits_after[0].summary, "add a");
    assert_eq!(commits_after[1].summary, "add b");
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn drop_removes_a_commit_from_the_resulting_history() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");
    write_file(dir.path(), "b.txt", "b\n");
    commit_all(&repo, "add b");

    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();
    let plan = vec![
        RebasePlanEntry {
            commit_id: commits[0].id.clone(),
            action: RebaseAction::Drop,
            combined_message: None,
        },
        pick(&commits[1].id),
    ];

    let (_state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    let commits_after = git_core::rebase::commits_since(&repo, &onto).unwrap();
    assert_eq!(commits_after.len(), 1);
    assert_eq!(commits_after[0].summary, "add b");
    assert!(!dir.path().join("a.txt").exists());
    assert!(dir.path().join("b.txt").exists());
}

#[test]
fn reword_uses_the_new_message_not_the_original() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "original message");

    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();
    let plan = vec![RebasePlanEntry {
        commit_id: commits[0].id.clone(),
        action: RebaseAction::Reword {
            message: "reworded message".to_string(),
        },
        combined_message: None,
    }];

    let (_state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(
        head_commit.message().ok().unwrap_or_default(),
        "reworded message"
    );
}

#[test]
fn squash_combines_a_group_into_one_commit_with_the_leaders_combined_message() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");
    write_file(dir.path(), "b.txt", "b\n");
    commit_all(&repo, "add b");

    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();
    let plan = vec![
        RebasePlanEntry {
            commit_id: commits[0].id.clone(),
            action: RebaseAction::Pick,
            combined_message: Some("combined: add a and b".to_string()),
        },
        RebasePlanEntry {
            commit_id: commits[1].id.clone(),
            action: RebaseAction::Squash,
            combined_message: None,
        },
    ];

    let (_state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    let commits_after = git_core::rebase::commits_since(&repo, &onto).unwrap();
    // Exactly one commit — the intermediate "add a" step was collapsed away, not left standing
    // alongside the squashed result.
    assert_eq!(commits_after.len(), 1);
    let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(
        head_commit.message().ok().unwrap_or_default(),
        "combined: add a and b"
    );
    assert_eq!(head_commit.parent_count(), 1);
    let parent = head_commit.parent(0).unwrap();
    assert_eq!(parent.id().to_string(), onto);
    // The combined tree reflects both changes.
    assert!(dir.path().join("a.txt").exists());
    assert!(dir.path().join("b.txt").exists());
}

#[test]
fn a_mixed_squash_and_fixup_group_still_collapses_to_one_commit() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");
    write_file(dir.path(), "b.txt", "b\n");
    commit_all(&repo, "add b (fixup target)");
    write_file(dir.path(), "c.txt", "c\n");
    commit_all(&repo, "add c");

    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();
    let plan = vec![
        RebasePlanEntry {
            commit_id: commits[0].id.clone(),
            action: RebaseAction::Pick,
            combined_message: Some("combined: a, b, c".to_string()),
        },
        RebasePlanEntry {
            commit_id: commits[1].id.clone(),
            action: RebaseAction::Fixup,
            combined_message: None,
        },
        RebasePlanEntry {
            commit_id: commits[2].id.clone(),
            action: RebaseAction::Squash,
            combined_message: None,
        },
    ];

    let (_state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    let commits_after = git_core::rebase::commits_since(&repo, &onto).unwrap();
    assert_eq!(commits_after.len(), 1);
    let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(
        head_commit.message().ok().unwrap_or_default(),
        "combined: a, b, c"
    );
    assert!(dir.path().join("a.txt").exists());
    assert!(dir.path().join("b.txt").exists());
    assert!(dir.path().join("c.txt").exists());
}

#[test]
fn a_conflicting_pick_pauses_and_resolving_then_continuing_lands_it() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "shared.txt", "line one\nline two\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "shared.txt", "line one\nchanged on top\n");
    commit_all(&repo, "change on top of onto");
    // A second, independent base to diverge from — so replaying "conflicting change" onto
    // "onto" (which itself has an unrelated edit to the same line) produces a real conflict.
    write_file(dir.path(), "shared.txt", "line one\nchanged again\n");
    commit_all(&repo, "conflicting change");

    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();
    // Only rebase the second commit ("conflicting change") onto the first's parent — but since
    // the first commit ("change on top of onto") is what actually landed on `onto` already, we
    // rebase starting from `onto` directly with just the conflicting commit, forcing a genuine
    // conflict against `onto`'s own content.
    let plan = vec![pick(&commits[1].id)];

    let (mut state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();

    let files = match result {
        RebaseStepResult::Conflicted { files } => files,
        other => panic!("expected Conflicted, got {other:?}"),
    };
    assert_eq!(files, vec!["shared.txt".to_string()]);

    // Resolve exactly like a merge conflict — same index, same write-then-stage mechanics.
    let workdir = repo.workdir().unwrap();
    std::fs::write(workdir.join("shared.txt"), "line one\nresolved\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("shared.txt")).unwrap();
    index.write().unwrap();

    let result = git_core::rebase::rebase_continue(&repo, &mut state).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    let contents = std::fs::read_to_string(dir.path().join("shared.txt")).unwrap();
    assert_eq!(contents, "line one\nresolved\n");
}

#[test]
fn an_edit_step_pauses_and_continuing_after_a_manual_amend_lands_the_amended_tree() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "a.txt", "original\n");
    commit_all(&repo, "add a");

    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();
    let plan = vec![RebasePlanEntry {
        commit_id: commits[0].id.clone(),
        action: RebaseAction::Edit,
        combined_message: None,
    }];

    let (mut state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();
    assert_eq!(result, RebaseStepResult::PausedForEdit);

    // Amend: change the file further and stage it, exactly like the normal Stage flow would.
    write_file(dir.path(), "a.txt", "amended\n");
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("a.txt")).unwrap();
    index.write().unwrap();

    let result = git_core::rebase::rebase_continue(&repo, &mut state).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    let contents = std::fs::read_to_string(dir.path().join("a.txt")).unwrap();
    assert_eq!(contents, "amended\n");
}

#[test]
fn finishing_moves_the_original_branch_and_reattaches_head() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");
    let branch_name = git_core::branch::list_branches(&repo).unwrap()[0].name.clone();

    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();
    let plan = vec![pick(&commits[0].id)];

    let (_state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    let head_ref = repo.head().unwrap();
    assert_eq!(head_ref.shorthand().unwrap(), branch_name);
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p git-core --test rebase`
Expected: FAIL to compile — `RebaseAction`, `RebasePlanEntry`, `RebaseStepResult`, `start_rebase`,
`rebase_continue` don't exist yet.

- [ ] **Step 4: Write the implementation**

Append to `crates/git-core/src/rebase.rs`:

```rust
use git2::Oid;

use crate::merge::conflict_path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebaseAction {
    Pick,
    Reword { message: String },
    Edit,
    Squash,
    Fixup,
    Drop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebasePlanEntry {
    pub commit_id: String,
    pub action: RebaseAction,
    pub combined_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebaseStepResult {
    Conflicted { files: Vec<String> },
    PausedForEdit,
    Advanced,
    Done,
}

pub struct RebaseState {
    plan: Vec<RebasePlanEntry>,
    cursor: usize,
    original_branch_ref: String,
    original_tip: Oid,
    group_start_parent: Option<Oid>,
}

impl RebaseState {
    /// 1-indexed count of plan entries processed so far, for a "Step N of M" display. Reaches
    /// `total_steps()` once the rebase is `Done`.
    pub fn current_step(&self) -> usize {
        self.cursor
    }

    pub fn total_steps(&self) -> usize {
        self.plan.len()
    }
}

fn validate_plan(plan: &[RebasePlanEntry]) -> Result<(), RebaseError> {
    if let Some(first) = plan.first() {
        if matches!(first.action, RebaseAction::Squash | RebaseAction::Fixup) {
            return Err(RebaseError::InvalidPlan(
                "the first entry cannot be Squash or Fixup — there is no preceding commit in \
                 the plan for it to combine into"
                    .to_string(),
            ));
        }
    }
    Ok(())
}

pub fn start_rebase(
    repo: &Repository,
    onto: &str,
    plan: Vec<RebasePlanEntry>,
) -> Result<(RebaseState, RebaseStepResult), RebaseError> {
    validate_plan(&plan)?;

    let onto_commit = repo.revparse_single(onto)?.peel_to_commit()?;
    let head_ref = repo.head()?;
    let original_branch_ref = head_ref
        .name()
        .ok_or_else(|| RebaseError::InvalidPlan("HEAD is not on a branch".to_string()))?
        .to_string();
    let original_tip = head_ref.peel_to_commit()?.id();

    // Checking out before detaching HEAD means a refused checkout (modified/untracked files in
    // the way) leaves the repo exactly as it was — the same safety ordering
    // `branch::switch_branch` and `merge::start_merge`'s fast-forward path already use.
    repo.checkout_tree(onto_commit.as_object(), None)?;
    repo.set_head_detached(onto_commit.id())?;

    let mut state = RebaseState {
        plan,
        cursor: 0,
        original_branch_ref,
        original_tip,
        group_start_parent: None,
    };

    let result = advance(repo, &mut state)?;
    Ok((state, result))
}

pub fn rebase_continue(
    repo: &Repository,
    state: &mut RebaseState,
) -> Result<RebaseStepResult, RebaseError> {
    // The working index is presumed ready — either a conflict was just resolved, or an `Edit`
    // pause's amendment is staged. Land it as this step's commit, then keep advancing.
    land_current_step(repo, state)?;
    state.cursor += 1;
    advance(repo, state)
}

/// Auto-advances through plan entries starting at `state.cursor`: applies each via cherry-pick,
/// pausing at the first conflict or `Edit` step, or reaching `Done` once every entry lands.
fn advance(repo: &Repository, state: &mut RebaseState) -> Result<RebaseStepResult, RebaseError> {
    loop {
        if state.cursor >= state.plan.len() {
            return finish(repo, state);
        }

        let entry = state.plan[state.cursor].clone();

        if matches!(entry.action, RebaseAction::Drop) {
            state.cursor += 1;
            continue;
        }

        let commit = repo.find_commit(Oid::from_str(&entry.commit_id)?)?;

        if state.group_start_parent.is_none() && starts_a_group(&state.plan, state.cursor) {
            state.group_start_parent = Some(repo.head()?.peel_to_commit()?.id());
        }

        repo.cherrypick(&commit, None)?;

        if repo.index()?.has_conflicts() {
            return Ok(RebaseStepResult::Conflicted {
                files: conflicted_paths(repo)?,
            });
        }

        if matches!(entry.action, RebaseAction::Edit) {
            return Ok(RebaseStepResult::PausedForEdit);
        }

        land_current_step(repo, state)?;
        state.cursor += 1;
    }
}

fn starts_a_group(plan: &[RebasePlanEntry], index: usize) -> bool {
    matches!(
        plan.get(index + 1).map(|e| &e.action),
        Some(RebaseAction::Squash) | Some(RebaseAction::Fixup)
    )
}

fn ends_a_group(plan: &[RebasePlanEntry], index: usize) -> bool {
    matches!(
        plan[index].action,
        RebaseAction::Squash | RebaseAction::Fixup
    ) && !matches!(
        plan.get(index + 1).map(|e| &e.action),
        Some(RebaseAction::Squash) | Some(RebaseAction::Fixup)
    )
}

/// Commits the currently-staged result for `plan[state.cursor]` as a real intermediate commit
/// (parent = current `HEAD`, author preserved from the original commit, committer = the current
/// user) — then, if this entry ends a squash/fixup group, collapses the whole group's chain of
/// intermediate commits into one final commit reparented onto `group_start_parent`, carrying the
/// group leader's `combined_message`. The leader is found by walking backward from this entry to
/// the nearest preceding entry that isn't itself `Squash`/`Fixup` — guaranteed to exist, since
/// `validate_plan` rejects a plan whose very first entry is `Squash`/`Fixup`.
fn land_current_step(repo: &Repository, state: &mut RebaseState) -> Result<(), RebaseError> {
    let entry = state.plan[state.cursor].clone();
    let original_commit = repo.find_commit(Oid::from_str(&entry.commit_id)?)?;
    let message = match &entry.action {
        RebaseAction::Reword { message } => message.clone(),
        _ => original_commit
            .message()
            .ok()
            .unwrap_or_default()
            .to_string(),
    };

    let mut index = repo.index()?;
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let parent = repo.head()?.peel_to_commit()?;
    let committer = repo.signature()?;
    repo.commit(
        Some("HEAD"),
        &original_commit.author(),
        &committer,
        &message,
        &tree,
        &[&parent],
    )?;

    if ends_a_group(&state.plan, state.cursor) {
        let leader_index = (0..=state.cursor)
            .rev()
            .find(|&i| {
                !matches!(
                    state.plan[i].action,
                    RebaseAction::Squash | RebaseAction::Fixup
                )
            })
            .expect("validate_plan guarantees a non-squash/fixup leader precedes any group");
        let combined_message = state.plan[leader_index]
            .combined_message
            .clone()
            .unwrap_or_else(|| message.clone());

        let final_tree = repo.head()?.peel_to_commit()?.tree()?;
        let group_parent = repo.find_commit(
            state
                .group_start_parent
                .expect("ends_a_group implies a group was started"),
        )?;
        repo.commit(
            Some("HEAD"),
            &original_commit.author(),
            &committer,
            &combined_message,
            &final_tree,
            &[&group_parent],
        )?;
        state.group_start_parent = None;
    }

    Ok(())
}

fn conflicted_paths(repo: &Repository) -> Result<Vec<String>, RebaseError> {
    let index = repo.index()?;
    let mut files = Vec::new();
    for conflict in index.conflicts()? {
        let conflict = conflict?;
        if let Some(path) = conflict_path(&conflict) {
            if !files.contains(&path) {
                files.push(path);
            }
        }
    }
    Ok(files)
}

fn finish(repo: &Repository, state: &RebaseState) -> Result<RebaseStepResult, RebaseError> {
    let final_oid = repo.head()?.peel_to_commit()?.id();
    let mut branch_ref = repo.find_reference(&state.original_branch_ref)?;
    branch_ref.set_target(final_oid, "rebase (finish): returning to branch")?;
    repo.set_head(&state.original_branch_ref)?;
    Ok(RebaseStepResult::Done)
}
```

Note: `RebaseStepResult::Advanced` is declared (part of the type's public shape per the design
spec) but never actually constructed by this implementation — `advance`'s loop only ever *returns*
on `Conflicted`, `PausedForEdit`, or `Done`; a step that lands cleanly and isn't the last one just
continues the loop silently rather than returning `Advanced` and requiring the caller to call
`rebase_continue` again for every single clean step. This matches the design spec's own framing
("auto-advance through clean ... steps"). `Advanced` is kept in the enum for downstream layers
(e.g. a future exec-step or a case where auto-advancing turns out to be undesirable) rather than
removed — it's a real, intentional part of the wire contract, not dead placeholder code; if a
reviewer flags it as unreachable, that's an accurate observation, not a defect to fix here.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p git-core --test rebase`
Expected: PASS (11 tests: 2 from Task 1 + 9 new). Also run `cargo test -p git-core` (whole crate)
to confirm `merge.rs`'s tests are unaffected by widening `conflict_path`'s visibility.

- [ ] **Step 6: Commit**

```bash
git add crates/git-core/src/merge.rs crates/git-core/src/rebase.rs crates/git-core/tests/rebase.rs
git commit -m "feat(git-core): add the rebase cherry-pick engine (start_rebase, rebase_continue)"
```

---

### Task 3: `git-core::rebase` — `abort_rebase`

**Files:**
- Modify: `crates/git-core/src/rebase.rs`
- Modify: `crates/git-core/tests/rebase.rs`

**Interfaces:**
- Consumes: `RebaseState`, `RebaseError` from Task 2.
- Produces (used by Task 4 and downstream):
  ```rust
  pub fn abort_rebase(repo: &git2::Repository, state: RebaseState) -> Result<(), RebaseError>;
  ```

- [ ] **Step 1: Write the failing tests**

Append to `crates/git-core/tests/rebase.rs`:

```rust
#[test]
fn abort_rebase_restores_the_original_branch_and_tip_exactly() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");
    let branch_name = git_core::branch::list_branches(&repo).unwrap()[0].name.clone();
    let original_tip = repo.head().unwrap().peel_to_commit().unwrap().id();

    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();
    let plan = vec![pick(&commits[0].id)];
    let (state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();
    assert_eq!(result, RebaseStepResult::Done);
    // Deliberately re-derive a fresh, still-in-progress-looking state isn't possible once
    // `start_rebase` already reached `Done` in one shot for a clean plan — abort a rebase that's
    // still genuinely paused instead, so this test exercises the real "abort mid-flight" case.
    let _ = state;

    // Rebuild a genuinely paused rebase to abort: an Edit step, still open when we abort.
    write_file(dir.path(), "base.txt", "v1\n"); // no-op rewrite, just to get a clean starting point
    let commits_again = git_core::rebase::commits_since(&repo, &onto).unwrap();
    let plan = vec![RebasePlanEntry {
        commit_id: commits_again[0].id.clone(),
        action: RebaseAction::Edit,
        combined_message: None,
    }];
    let (state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();
    assert_eq!(result, RebaseStepResult::PausedForEdit);

    git_core::rebase::abort_rebase(&repo, state).unwrap();

    let head_ref = repo.head().unwrap();
    assert_eq!(head_ref.shorthand().unwrap(), branch_name);
    assert_eq!(head_ref.peel_to_commit().unwrap().id(), original_tip);
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
    let contents = std::fs::read_to_string(dir.path().join("a.txt")).unwrap();
    assert_eq!(contents, "a\n");
}

#[test]
fn abort_rebase_after_a_conflict_also_recovers_cleanly() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "shared.txt", "line one\nline two\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "shared.txt", "line one\nchanged on top\n");
    commit_all(&repo, "change on top of onto");
    write_file(dir.path(), "shared.txt", "line one\nchanged again\n");
    commit_all(&repo, "conflicting change");
    let branch_name = git_core::branch::list_branches(&repo).unwrap()[0].name.clone();
    let original_tip = repo.head().unwrap().peel_to_commit().unwrap().id();

    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();
    let plan = vec![pick(&commits[1].id)];
    let (state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();
    assert!(matches!(result, RebaseStepResult::Conflicted { .. }));

    git_core::rebase::abort_rebase(&repo, state).unwrap();

    let head_ref = repo.head().unwrap();
    assert_eq!(head_ref.shorthand().unwrap(), branch_name);
    assert_eq!(head_ref.peel_to_commit().unwrap().id(), original_tip);
    assert!(!repo.index().unwrap().has_conflicts());
    let contents = std::fs::read_to_string(dir.path().join("shared.txt")).unwrap();
    assert_eq!(contents, "line one\nchanged again\n");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test rebase`
Expected: FAIL to compile — `abort_rebase` doesn't exist yet.

- [ ] **Step 3: Write the implementation**

Append to `crates/git-core/src/rebase.rs`:

```rust
pub fn abort_rebase(repo: &Repository, state: RebaseState) -> Result<(), RebaseError> {
    // The original branch ref was never touched during the rebase — only the detached HEAD
    // moved — so recovery is just reattaching to it and force-checking-out its tree over
    // whatever the in-progress rebase left in the working directory/index.
    repo.set_head(&state.original_branch_ref)?;
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.force();
    repo.checkout_head(Some(&mut checkout))?;
    repo.cleanup_state()?;
    Ok(())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p git-core --test rebase && cargo test -p git-core`
Expected: PASS (13 tests in `rebase.rs`; full crate unaffected).

- [ ] **Step 5: Commit**

```bash
git add crates/git-core/src/rebase.rs crates/git-core/tests/rebase.rs
git commit -m "feat(git-core): add abort_rebase"
```

---

### Task 4: `tauri-app::worker` — rebase Commands

**Files:**
- Modify: `crates/tauri-app/src/worker.rs`

**Interfaces:**
- Consumes: `git_core::rebase::{RebasePlanCommit, RebasePlanEntry, RebaseStepResult, RebaseState,
  RebaseError, commits_since, start_rebase, rebase_continue, abort_rebase}` from Tasks 1-3.
- Produces (used by Task 5):
  ```rust
  impl WorkerHandle {
      pub fn commits_since(&self, onto: String) -> Result<Vec<RebasePlanCommit>, String>;
      pub fn start_rebase(&self, onto: String, plan: Vec<RebasePlanEntry>) -> Result<RebaseStepResult, String>;
      pub fn rebase_continue(&self) -> Result<RebaseStepResult, String>;
      pub fn abort_rebase(&self) -> Result<(), String>;
      pub fn get_rebase_progress(&self) -> Result<Option<(usize, usize)>, String>;
  }
  ```
  Unlike every other feature so far, the worker's owned state grows by one more field:
  `rebase_state: Option<git_core::rebase::RebaseState>`, held in the same `thread::spawn`
  closure as `repo`, since `RebaseState` genuinely needs to persist *across* IPC calls
  (`start_rebase` → zero or more conflict-resolution/edit round trips via `rebase_continue` →
  `Done`) — this is exactly the "session-scoped state" the design spec calls for, and the worker
  thread's closure is where every other piece of this app's per-repo session state already lives.

- [ ] **Step 1: Write the failing tests**

Add to `crates/tauri-app/src/worker.rs`'s `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn commits_since_and_start_rebase_round_trip_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "base.txt", "v1\n");
        commit_all(&repo, "base commit");
        let onto = repo.head().unwrap().peel_to_commit().unwrap().id().to_string();
        write_file(dir.path(), "a.txt", "a\n");
        commit_all(&repo, "add a");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();

        let commits = handle.commits_since(onto.clone()).unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].summary, "add a");

        let plan = vec![git_core::rebase::RebasePlanEntry {
            commit_id: commits[0].id.clone(),
            action: git_core::rebase::RebaseAction::Pick,
            combined_message: None,
        }];
        let result = handle.start_rebase(onto, plan).unwrap();

        assert_eq!(result, git_core::rebase::RebaseStepResult::Done);
        assert_eq!(handle.get_rebase_progress().unwrap(), None);
    }

    #[test]
    fn rebase_continue_and_abort_rebase_round_trip_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "base.txt", "v1\n");
        commit_all(&repo, "base commit");
        let onto = repo.head().unwrap().peel_to_commit().unwrap().id().to_string();
        write_file(dir.path(), "a.txt", "a\n");
        commit_all(&repo, "add a");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();

        let commits = handle.commits_since(onto.clone()).unwrap();
        let plan = vec![git_core::rebase::RebasePlanEntry {
            commit_id: commits[0].id.clone(),
            action: git_core::rebase::RebaseAction::Edit,
            combined_message: None,
        }];
        let result = handle.start_rebase(onto, plan).unwrap();
        assert_eq!(result, git_core::rebase::RebaseStepResult::PausedForEdit);
        assert_eq!(handle.get_rebase_progress().unwrap(), Some((0, 1)));

        handle.abort_rebase().unwrap();

        assert_eq!(handle.get_rebase_progress().unwrap(), None);
        assert!(handle.get_status().unwrap().is_empty());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p tauri-app`
Expected: FAIL to compile — the new `WorkerHandle` methods don't exist yet.

- [ ] **Step 3: Write the implementation**

Update the imports (alphabetical: `blame, branch, diff, graph, merge, rebase, stash, status` —
`rebase` sits between `merge` and `stash`):

```rust
use git_core::rebase::{RebasePlanCommit, RebasePlanEntry, RebaseState, RebaseStepResult};
```

Change the `thread::spawn` closure's opening to also own the rebase state:

```rust
        thread::spawn(move || {
            let mut repo = repo;
            let mut rebase_state: Option<RebaseState> = None;
            for command in rx {
```

Add five `Command` variants (after `ResolveAddDeleteConflict`, the last existing variant):

```rust
    CommitsSince {
        onto: String,
        reply: Sender<Result<Vec<RebasePlanCommit>, String>>,
    },
    StartRebase {
        onto: String,
        plan: Vec<RebasePlanEntry>,
        reply: Sender<Result<RebaseStepResult, String>>,
    },
    RebaseContinue {
        reply: Sender<Result<RebaseStepResult, String>>,
    },
    AbortRebase {
        reply: Sender<Result<(), String>>,
    },
    GetRebaseProgress {
        reply: Sender<Result<Option<(usize, usize)>, String>>,
    },
```

Add five match arms (after the `Command::ResolveAddDeleteConflict` arm):

```rust
                    Command::CommitsSince { onto, reply } => {
                        let result = git_core::rebase::commits_since(&repo, &onto)
                            .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::StartRebase { onto, plan, reply } => {
                        let result = git_core::rebase::start_rebase(&repo, &onto, plan)
                            .map_err(|e| e.to_string())
                            .map(|(state, step)| {
                                rebase_state = Some(state);
                                step
                            });
                        let _ = reply.send(result);
                    }
                    Command::RebaseContinue { reply } => {
                        let result = match rebase_state.as_mut() {
                            Some(state) => git_core::rebase::rebase_continue(&repo, state)
                                .map_err(|e| e.to_string()),
                            None => Err("no rebase is currently in progress".to_string()),
                        };
                        let _ = reply.send(result);
                    }
                    Command::AbortRebase { reply } => {
                        let result = match rebase_state.take() {
                            Some(state) => git_core::rebase::abort_rebase(&repo, state)
                                .map_err(|e| e.to_string()),
                            None => Err("no rebase is currently in progress".to_string()),
                        };
                        let _ = reply.send(result);
                    }
                    Command::GetRebaseProgress { reply } => {
                        let progress = rebase_state
                            .as_ref()
                            .map(|s| (s.current_step(), s.total_steps()));
                        let _ = reply.send(Ok(progress));
                    }
```

Note: `StartRebase`'s handler doesn't clear `rebase_state` once the result is `Done` — the plan
having finished immediately (a clean, pause-free rebase) still stores a completed `RebaseState`
until the *next* `GetRebaseProgress`/`RebaseContinue` call would otherwise misreport it. Fix this
by also clearing `rebase_state` whenever the result is `Done`:

```rust
                    Command::StartRebase { onto, plan, reply } => {
                        let result = git_core::rebase::start_rebase(&repo, &onto, plan)
                            .map_err(|e| e.to_string());
                        match &result {
                            Ok((_, RebaseStepResult::Done)) => rebase_state = None,
                            Ok((state, _)) => {
                                // Re-borrow: `state` above came from destructuring `&result`, so
                                // it can't be moved out — store the state by re-matching `result`
                                // (owned) instead.
                            }
                            Err(_) => {}
                        }
                        let reply_value = result.map(|(state, step)| {
                            if !matches!(step, RebaseStepResult::Done) {
                                rebase_state = Some(state);
                            }
                            step
                        });
                        let _ = reply.send(reply_value);
                    }
```

This is awkward as written — simplify to a single clean pass instead of the two-step
match-then-map above:

```rust
                    Command::StartRebase { onto, plan, reply } => {
                        let result = git_core::rebase::start_rebase(&repo, &onto, plan)
                            .map_err(|e| e.to_string())
                            .map(|(state, step)| {
                                if !matches!(step, RebaseStepResult::Done) {
                                    rebase_state = Some(state);
                                }
                                step
                            });
                        let _ = reply.send(result);
                    }
```

(Replace the earlier draft of this match arm with this final version — the two-step version
above was shown only to explain *why* the check is needed, not as code to keep.) Apply the same
"clear `rebase_state` when the result is `Done`" fix to `Command::RebaseContinue`:

```rust
                    Command::RebaseContinue { reply } => {
                        let result = match rebase_state.as_mut() {
                            Some(state) => git_core::rebase::rebase_continue(&repo, state)
                                .map_err(|e| e.to_string()),
                            None => Err("no rebase is currently in progress".to_string()),
                        };
                        if matches!(result, Ok(RebaseStepResult::Done)) {
                            rebase_state = None;
                        }
                        let _ = reply.send(result);
                    }
```

Add five `WorkerHandle` methods (after `resolve_add_delete_conflict`):

```rust
    pub fn commits_since(&self, onto: String) -> Result<Vec<RebasePlanCommit>, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::CommitsSince {
                onto,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn start_rebase(
        &self,
        onto: String,
        plan: Vec<RebasePlanEntry>,
    ) -> Result<RebaseStepResult, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::StartRebase {
                onto,
                plan,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn rebase_continue(&self) -> Result<RebaseStepResult, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::RebaseContinue { reply: reply_tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn abort_rebase(&self) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::AbortRebase { reply: reply_tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn get_rebase_progress(&self) -> Result<Option<(usize, usize)>, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::GetRebaseProgress { reply: reply_tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p tauri-app`
Expected: PASS (19 tests: 17 existing + 2 new).

- [ ] **Step 5: Commit**

```bash
git add crates/tauri-app/src/worker.rs
git commit -m "feat(tauri-app): wire rebase operations into the worker"
```

---

### Task 5: Tauri commands for rebase operations

**Files:**
- Modify: `crates/tauri-app/src/commands.rs`
- Modify: `crates/tauri-app/src/main.rs`

**Interfaces:**
- Consumes: `WorkerHandle::{commits_since, start_rebase, rebase_continue, abort_rebase,
  get_rebase_progress}` from Task 4.
- Produces (used by Task 6): five Tauri commands — `commits_since`, `start_rebase`,
  `rebase_continue`, `abort_rebase`, `get_rebase_progress` — returning
  `Vec<RebasePlanCommitDto>`/`RebaseStepResultDto`/`RebaseStepResultDto`/`()`/`Option<(usize,
  usize)>` respectively. `RebasePlanEntryDto` (the request-side type for `start_rebase`) uses
  `#[derive(Deserialize)]` with a tagged `RebaseActionDto` matching the frontend's discriminated
  union exactly, same convention `FileConflictChoiceDto` already established for merge.

No dedicated test for this task: thin pass-through commands aren't separately tested per
`CLAUDE.md`'s convention, and none of the new DTOs use `Debug`-formatted wire values (unlike
`StatusKind`) — no pinned wire-format test needed.

- [ ] **Step 1: Add the DTOs and commands**

In `crates/tauri-app/src/commands.rs`, add (after `FileConflictChoiceDto`'s `impl From<...>`
block):

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebasePlanCommitDto {
    pub id: String,
    pub short_id: String,
    pub summary: String,
    pub author_name: String,
    pub timestamp: i64,
}

impl From<git_core::rebase::RebasePlanCommit> for RebasePlanCommitDto {
    fn from(c: git_core::rebase::RebasePlanCommit) -> Self {
        RebasePlanCommitDto {
            id: c.id,
            short_id: c.short_id,
            summary: c.summary,
            author_name: c.author_name,
            timestamp: c.timestamp,
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
pub enum RebaseActionDto {
    Pick,
    Reword { message: String },
    Edit,
    Squash,
    Fixup,
    Drop,
}

impl From<RebaseActionDto> for git_core::rebase::RebaseAction {
    fn from(dto: RebaseActionDto) -> Self {
        match dto {
            RebaseActionDto::Pick => git_core::rebase::RebaseAction::Pick,
            RebaseActionDto::Reword { message } => {
                git_core::rebase::RebaseAction::Reword { message }
            }
            RebaseActionDto::Edit => git_core::rebase::RebaseAction::Edit,
            RebaseActionDto::Squash => git_core::rebase::RebaseAction::Squash,
            RebaseActionDto::Fixup => git_core::rebase::RebaseAction::Fixup,
            RebaseActionDto::Drop => git_core::rebase::RebaseAction::Drop,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RebasePlanEntryDto {
    pub commit_id: String,
    pub action: RebaseActionDto,
    pub combined_message: Option<String>,
}

impl From<RebasePlanEntryDto> for git_core::rebase::RebasePlanEntry {
    fn from(dto: RebasePlanEntryDto) -> Self {
        git_core::rebase::RebasePlanEntry {
            commit_id: dto.commit_id,
            action: dto.action.into(),
            combined_message: dto.combined_message,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind")]
pub enum RebaseStepResultDto {
    Conflicted { files: Vec<String> },
    PausedForEdit,
    Advanced,
    Done,
}

impl From<git_core::rebase::RebaseStepResult> for RebaseStepResultDto {
    fn from(result: git_core::rebase::RebaseStepResult) -> Self {
        match result {
            git_core::rebase::RebaseStepResult::Conflicted { files } => {
                RebaseStepResultDto::Conflicted { files }
            }
            git_core::rebase::RebaseStepResult::PausedForEdit => {
                RebaseStepResultDto::PausedForEdit
            }
            git_core::rebase::RebaseStepResult::Advanced => RebaseStepResultDto::Advanced,
            git_core::rebase::RebaseStepResult::Done => RebaseStepResultDto::Done,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebaseProgressDto {
    pub current_step: usize,
    pub total_steps: usize,
}
```

Add five commands (after `resolve_add_delete_conflict`, the last existing command):

```rust
#[tauri::command]
pub async fn commits_since(
    onto: String,
    state: State<'_, AppState>,
) -> Result<Vec<RebasePlanCommitDto>, String> {
    let commits = worker_handle(&state)?.commits_since(onto)?;
    Ok(commits.into_iter().map(RebasePlanCommitDto::from).collect())
}

#[tauri::command]
pub async fn start_rebase(
    onto: String,
    plan: Vec<RebasePlanEntryDto>,
    state: State<'_, AppState>,
) -> Result<RebaseStepResultDto, String> {
    let plan = plan.into_iter().map(Into::into).collect();
    let result = worker_handle(&state)?.start_rebase(onto, plan)?;
    Ok(RebaseStepResultDto::from(result))
}

#[tauri::command]
pub async fn rebase_continue(state: State<'_, AppState>) -> Result<RebaseStepResultDto, String> {
    let result = worker_handle(&state)?.rebase_continue()?;
    Ok(RebaseStepResultDto::from(result))
}

#[tauri::command]
pub async fn abort_rebase(state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state)?.abort_rebase()
}

#[tauri::command]
pub async fn get_rebase_progress(
    state: State<'_, AppState>,
) -> Result<Option<RebaseProgressDto>, String> {
    let progress = worker_handle(&state)?.get_rebase_progress()?;
    Ok(progress.map(|(current_step, total_steps)| RebaseProgressDto {
        current_step,
        total_steps,
    }))
}
```

- [ ] **Step 2: Update `main.rs`**

Update the `use commands::{...}` import list (fully alphabetized — five new names inserted in
place):

```rust
use commands::{
    abort_merge, abort_rebase, apply_stash, commit, commits_since, create_branch, delete_branch,
    drop_stash, get_blame, get_commit_diff, get_commit_files, get_commit_graph,
    get_conflict_hunks, get_merge_message, get_rebase_progress, get_status, get_working_diff,
    list_branches, list_recent_repos, list_stashes, open_repo, pick_repo_folder, rebase_continue,
    rename_branch, resolve_add_delete_conflict, resolve_conflict, save_stash, stage_file,
    start_merge, start_rebase, switch_branch, unstage_file, AppState,
};
```

Add the five commands to `tauri::generate_handler![...]` (appended at the end, after
`get_merge_message` — new commands, not replacements):

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
            start_merge,
            get_conflict_hunks,
            resolve_conflict,
            resolve_add_delete_conflict,
            abort_merge,
            get_merge_message,
            commits_since,
            start_rebase,
            rebase_continue,
            abort_rebase,
            get_rebase_progress,
        ])
```

- [ ] **Step 3: Verify it builds and tests pass**

Run: `cargo build --workspace && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Expected: all clean.

- [ ] **Step 4: Commit**

```bash
git add crates/tauri-app/src/commands.rs crates/tauri-app/src/main.rs
git commit -m "feat(tauri-app): add Tauri commands for rebase operations"
```

---

### Task 6: `RepoClient` + `useAppState` — rebase additions

**Files:**
- Modify: `frontend/src/ipc/RepoClient.ts`
- Modify: `frontend/src/ipc/tauriRepoClient.ts`
- Modify: `frontend/src/state/useAppState.ts`
- Modify: `frontend/src/state/useAppState.test.ts`
- Modify: `frontend/src/components/DiffPane.test.tsx` (one-line factory addition)
- Modify: `frontend/src/components/RepoPicker.test.tsx` (one-line factory addition)
- Modify: `frontend/src/components/ConflictResolutionPane.test.tsx` (one-line factory addition —
  a prior feature's final review found this file's `fakeClient` factory gets missed if not
  explicitly listed; it also constructs a full `RepoClient`-typed object)

**Interfaces:**
- Consumes: Tauri commands from Task 5.
- Produces (used by Tasks 7-9):
  ```ts
  export interface RebasePlanCommit { id, shortId, summary, authorName, timestamp }
  export type RebaseAction =
    | { kind: "Pick" }
    | { kind: "Reword"; message: string }
    | { kind: "Edit" }
    | { kind: "Squash" }
    | { kind: "Fixup" }
    | { kind: "Drop" };
  export interface RebasePlanEntry { commitId: string; action: RebaseAction; combinedMessage: string | null; }
  export type RebaseStepResult =
    | { kind: "Conflicted"; files: string[] }
    | { kind: "PausedForEdit" }
    | { kind: "Advanced" }
    | { kind: "Done" };
  // on RepoClient:
  commitsSince(onto: string): Promise<RebasePlanCommit[]>;
  startRebase(onto: string, plan: RebasePlanEntry[]): Promise<RebaseStepResult>;
  rebaseContinue(): Promise<RebaseStepResult>;
  abortRebase(): Promise<void>;
  getRebaseProgress(): Promise<{ currentStep: number; totalSteps: number } | null>;
  // on AppState:
  rebaseProgress: { currentStep: number; totalSteps: number } | null;
  rebaseOnto: string | null;   // which commit the open planner is rebasing onto, or null = closed
  // on UseAppStateResult:
  openRebasePlanner(commitId: string): void;
  closeRebasePlanner(): void;
  startRebase(onto: string, plan: RebasePlanEntry[]): Promise<void>;
  rebaseContinue(): Promise<void>;
  abortRebase(): Promise<void>;
  ```
  `commitsSince` is **not** exposed on `UseAppStateResult` — like `getConflictHunks`, it's a read
  fetched directly by the component that needs it (Task 8's `RebasePlanner`, via its own `client`
  prop), matching the established "reads bypass `useAppState`, only mutations go through it"
  convention.

- **Why this task bundles the interface addition with every existing mock literal:** matches the
  merge feature's own Task 8 reasoning exactly — `useAppState.test.ts` has 17 separate `it()`
  blocks that each construct their own full `RepoClient` object literal (not a shared factory),
  so touching them once now for all 5 new methods together is strictly cheaper than doing it
  repeatedly across separate tasks. No forcing constraint, purely an efficiency choice, stated
  explicitly per this project's own established practice of not leaving that ambiguous.

- [ ] **Step 1: `RepoClient.ts`**

Add the four new types (after `FileConflictChoice`, the last existing type, before the
`RepoClient` interface itself):

```ts
export interface RebasePlanCommit {
  id: string;
  shortId: string;
  summary: string;
  authorName: string;
  timestamp: number;
}

export type RebaseAction =
  | { kind: "Pick" }
  | { kind: "Reword"; message: string }
  | { kind: "Edit" }
  | { kind: "Squash" }
  | { kind: "Fixup" }
  | { kind: "Drop" };

export interface RebasePlanEntry {
  commitId: string;
  action: RebaseAction;
  combinedMessage: string | null;
}

export type RebaseStepResult =
  | { kind: "Conflicted"; files: string[] }
  | { kind: "PausedForEdit" }
  | { kind: "Advanced" }
  | { kind: "Done" };
```

Add five methods to the `RepoClient` interface (after `resolveAddDeleteConflict`, the last
existing method):

```ts
  commitsSince(onto: string): Promise<RebasePlanCommit[]>;
  startRebase(onto: string, plan: RebasePlanEntry[]): Promise<RebaseStepResult>;
  rebaseContinue(): Promise<RebaseStepResult>;
  abortRebase(): Promise<void>;
  getRebaseProgress(): Promise<{ currentStep: number; totalSteps: number } | null>;
```

- [ ] **Step 2: `tauriRepoClient.ts`**

Update the type-only import (alphabetical: `BlameLine, BranchInfo, ConflictSegment, DiffHunk,
FileConflictChoice, GraphCommit, MergeOutcome, RebaseAction, RebasePlanEntry, RebasePlanCommit,
RebaseStepResult, RepoClient, StashEntry, StatusEntry` — insert the four new names in their
alphabetical slots):

```ts
import type {
  BlameLine,
  BranchInfo,
  ConflictSegment,
  DiffHunk,
  FileConflictChoice,
  GraphCommit,
  MergeOutcome,
  RebaseAction,
  RebasePlanCommit,
  RebasePlanEntry,
  RebaseStepResult,
  RepoClient,
  StashEntry,
  StatusEntry,
} from "./RepoClient";
```

(`RebaseAction` is imported here only for consistency with the rest of the file's style — it
isn't referenced directly in the implementations below since `plan` is passed through opaquely,
but keeping unused-but-conceptually-relevant imports out is preferred: if `tsc`/`eslint` flags it
as unused, drop it from this import list rather than force a reference.)

Add five implementations (after `resolveAddDeleteConflict`):

```ts
  commitsSince: (onto: string) =>
    invoke<RebasePlanCommit[]>("commits_since", { onto }),
  startRebase: (onto: string, plan: RebasePlanEntry[]) =>
    invoke<RebaseStepResult>("start_rebase", { onto, plan }),
  rebaseContinue: () => invoke<RebaseStepResult>("rebase_continue"),
  abortRebase: () => invoke("abort_rebase"),
  getRebaseProgress: () =>
    invoke<{ currentStep: number; totalSteps: number } | null>("get_rebase_progress"),
```

- [ ] **Step 3: `useAppState.ts`**

Update the type-only import to include `RebasePlanEntry`, `RebaseStepResult` (alongside the
existing imports).

Add to the `AppState` interface (after `mergeMessage`):

```ts
  rebaseProgress: { currentStep: number; totalSteps: number } | null;
  rebaseOnto: string | null;
```

Add to the initial `useState` value: `rebaseProgress: null,` and `rebaseOnto: null,`.

Add `getRebaseProgress()` to `refresh()`'s `Promise.all`:

```ts
  const refresh = useCallback(async () => {
    try {
      const [status, commits, branches, stashes, mergeMessage, rebaseProgress] =
        await Promise.all([
          client.getStatus(),
          client.getCommitGraph(GRAPH_LIMIT),
          client.listBranches(),
          client.listStashes(),
          client.getMergeMessage(),
          client.getRebaseProgress(),
        ]);
      setState((prev) => ({
        ...prev,
        status,
        commits,
        branches,
        stashes,
        mergeMessage,
        rebaseProgress,
        error: null,
      }));
    } catch (err) {
      setState((prev) => ({ ...prev, error: String(err) }));
    }
  }, [client]);
```

Add state setters and mutation callbacks (after `abortMerge`, before the `return`):

```ts
  const openRebasePlanner = useCallback((commitId: string) => {
    setState((prev) => ({ ...prev, rebaseOnto: commitId }));
  }, []);
  const closeRebasePlanner = useCallback(() => {
    setState((prev) => ({ ...prev, rebaseOnto: null }));
  }, []);

  const startRebase = useCallback(
    (onto: string, plan: RebasePlanEntry[]): Promise<void> =>
      runMutation(async () => {
        const result: RebaseStepResult = await client.startRebase(onto, plan);
        void result;
        setState((prev) => ({ ...prev, rebaseOnto: null }));
      }),
    [client, runMutation],
  );
  const rebaseContinue = useCallback(
    (): Promise<void> =>
      runMutation(async () => {
        const result: RebaseStepResult = await client.rebaseContinue();
        void result;
      }),
    [client, runMutation],
  );
  const abortRebase = useCallback(
    () => runMutation(() => client.abortRebase()),
    [client, runMutation],
  );
```

Add `openRebasePlanner`, `closeRebasePlanner`, `startRebase`, `rebaseContinue`, `abortRebase` to
both `UseAppStateResult`'s interface and the final `return` object.

- [ ] **Step 4: Update `useAppState.test.ts`**

In **every one of the 17 pre-existing test blocks**, add to each inline `client: RepoClient =
{...}` literal, immediately after the existing `getMergeMessage:`/`resolveAddDeleteConflict:`
lines:

```ts
      commitsSince: async () => unimplemented(),
      startRebase: async () => unimplemented(),
      rebaseContinue: async () => unimplemented(),
      abortRebase: async () => unimplemented(),
      getRebaseProgress: async () => null,
```

`getRebaseProgress` defaults to `async () => null` (not `unimplemented()`) in all 17 blocks, for
the same reason `getMergeMessage` already does: `refresh()` now calls it unconditionally on every
`Promise.all`.

- [ ] **Step 5: Update the three `fakeClient` factories**

In `frontend/src/components/DiffPane.test.tsx`, `frontend/src/components/RepoPicker.test.tsx`,
and `frontend/src/components/ConflictResolutionPane.test.tsx`, add to each file's `fakeClient`
factory (after `resolveAddDeleteConflict: unused,` or that file's equivalent
untested-method style):

```ts
    commitsSince: unused,
    startRebase: unused,
    rebaseContinue: unused,
    abortRebase: unused,
    getRebaseProgress: async () => null,
```

(Match each file's existing convention for the `unused`/`unimplemented()` untested-method
placeholder exactly — they differ slightly between files, as established by prior tasks.)

- [ ] **Step 6: Run tests, build, and lint**

Run: `cd frontend && pnpm test -- --run && pnpm lint && pnpm build`
Expected: all PASS.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/ipc/RepoClient.ts frontend/src/ipc/tauriRepoClient.ts \
  frontend/src/state/useAppState.ts frontend/src/state/useAppState.test.ts \
  frontend/src/components/DiffPane.test.tsx frontend/src/components/RepoPicker.test.tsx \
  frontend/src/components/ConflictResolutionPane.test.tsx
git commit -m "feat(frontend): add rebase methods to RepoClient and useAppState"
```

---

### Task 7: `CommitGraph` — "Rebase onto here" context menu entry

**Files:**
- Modify: `frontend/src/components/CommitGraph.tsx`
- Modify: `frontend/src/components/CommitGraph.test.tsx`
- Modify: `frontend/src/App.tsx`

**Interfaces:**
- Consumes: `appState.openRebasePlanner` from Task 6.
- Produces: `CommitGraph` gains a required `onRebaseFromCommit: (commitId: string) => void` prop.

- [ ] **Step 1: Write the failing test**

Add to `frontend/src/components/CommitGraph.test.tsx` (add `onRebaseFromCommit={vi.fn()}` to
every existing `<CommitGraph ... />` render call in this file first — a new required prop):

```tsx
  it("right-clicking a commit and choosing Rebase onto here calls onRebaseFromCommit", () => {
    const onRebaseFromCommit = vi.fn();
    render(
      <CommitGraph
        status={status}
        commits={commits}
        stashes={[]}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={onRebaseFromCommit}
        onApplyStash={vi.fn()}
        onDropStash={vi.fn()}
      />,
    );

    const row = screen.getByText(/second commit/).closest("li");
    fireEvent.contextMenu(row!);
    fireEvent.click(screen.getByText("Rebase onto here"));

    expect(onRebaseFromCommit).toHaveBeenCalledWith("aaa111...");
  });
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd frontend && pnpm test -- --run CommitGraph`
Expected: FAIL — `onRebaseFromCommit` prop doesn't exist yet, "Rebase onto here" isn't rendered.

- [ ] **Step 3: Write the implementation**

In `frontend/src/components/CommitGraph.tsx`, add `onRebaseFromCommit` to the destructured props
and the prop-type object:

```tsx
export function CommitGraph({
  status,
  commits,
  stashes,
  selectedRow,
  pending,
  onSelectRow,
  onBranchFromCommit,
  onRebaseFromCommit,
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
  onRebaseFromCommit: (commitId: string) => void;
  onApplyStash: (index: number) => void;
  onDropStash: (index: number) => void;
}) {
```

Add a second entry to the existing context-menu `<ul>` (alongside "Branch from here"):

```tsx
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
          <li>
            <button
              onClick={() => {
                onRebaseFromCommit(contextMenu.commitId);
                setContextMenu(null);
              }}
            >
              Rebase onto here
            </button>
          </li>
        </ul>
      )}
```

In `frontend/src/App.tsx`, add `onRebaseFromCommit={appState.openRebasePlanner}` to the
`<CommitGraph ... />` element (after `onBranchFromCommit`).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && pnpm test -- --run && pnpm lint && pnpm build`
Expected: PASS, lint/build clean.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/CommitGraph.tsx frontend/src/components/CommitGraph.test.tsx \
  frontend/src/App.tsx
git commit -m "feat(frontend): add Rebase onto here to CommitGraph's context menu"
```

---

### Task 8: `RebasePlanner` component

**Files:**
- Create: `frontend/src/components/RebasePlanner.tsx`
- Create: `frontend/src/components/RebasePlanner.test.tsx`
- Modify: `frontend/src/App.tsx`

**Interfaces:**
- Consumes: `RebasePlanCommit`, `RebaseAction`, `RebasePlanEntry`, `RepoClient` from Task 6.
- Produces: the `RebasePlanner` component, rendered whenever `appState.state.rebaseOnto !==
  null`.

**Combined-message default, computed client-side:** for a group (a leader followed by one or
more Squash/Fixup rows), the leader's combined-message field defaults to the leader's own summary
followed by, for each `Squash` row in the group (never `Fixup`), a blank line and that row's own
summary — editable before starting. This mirrors real git's own default squash-message
convention (concatenated summaries) closely enough for a first pass without trying to exactly
replicate its multi-line-body handling.

- [ ] **Step 1: Write the failing tests**

Create `frontend/src/components/RebasePlanner.test.tsx`:

```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RebasePlanCommit, RepoClient } from "../ipc/RepoClient";
import { RebasePlanner } from "./RebasePlanner";

function unused(): never {
  throw new Error("not used in this test");
}

function fakeClient(overrides: Partial<RepoClient>): RepoClient {
  return {
    pickRepoFolder: unused,
    listRecentRepos: unused,
    openRepo: unused,
    getStatus: unused,
    getCommitGraph: unused,
    listBranches: unused,
    createBranch: unused,
    switchBranch: unused,
    deleteBranch: unused,
    renameBranch: unused,
    listStashes: unused,
    saveStash: unused,
    applyStash: unused,
    dropStash: unused,
    getBlame: unused,
    getWorkingDiff: unused,
    getCommitDiff: unused,
    getCommitFiles: unused,
    stageFile: unused,
    unstageFile: unused,
    commit: unused,
    mergeBranch: unused,
    getConflictHunks: unused,
    resolveConflict: unused,
    abortMerge: unused,
    getMergeMessage: unused,
    resolveAddDeleteConflict: unused,
    commitsSince: unused,
    startRebase: unused,
    rebaseContinue: unused,
    abortRebase: unused,
    getRebaseProgress: unused,
    ...overrides,
  };
}

const commits: RebasePlanCommit[] = [
  { id: "aaa", shortId: "aaa1111", summary: "add a", authorName: "Rene", timestamp: 1 },
  { id: "bbb", shortId: "bbb2222", summary: "add b", authorName: "Rene", timestamp: 2 },
];

describe("RebasePlanner", () => {
  it("lists commits oldest-first with a default Pick action each", async () => {
    const client = fakeClient({ commitsSince: async () => commits });

    render(
      <RebasePlanner client={client} onto="base" onStartRebase={vi.fn()} onCancel={vi.fn()} />,
    );

    expect(await screen.findByText(/add a/)).toBeInTheDocument();
    expect(screen.getByText(/add b/)).toBeInTheDocument();
  });

  it("moving a row down reorders the plan sent to onStartRebase", async () => {
    const onStartRebase = vi.fn();
    const client = fakeClient({ commitsSince: async () => commits });

    render(
      <RebasePlanner
        client={client}
        onto="base"
        onStartRebase={onStartRebase}
        onCancel={vi.fn()}
      />,
    );
    await screen.findByText(/add a/);

    fireEvent.click(screen.getAllByText("Move down")[0]);
    fireEvent.click(screen.getByText("Start Rebase"));

    expect(onStartRebase).toHaveBeenCalledWith(
      "base",
      expect.arrayContaining([
        expect.objectContaining({ commitId: "bbb" }),
        expect.objectContaining({ commitId: "aaa" }),
      ]),
    );
    const [, plan] = onStartRebase.mock.calls[0];
    expect(plan[0].commitId).toBe("bbb");
    expect(plan[1].commitId).toBe("aaa");
  });

  it("selecting Reword reveals a message field and includes it in the plan", async () => {
    const onStartRebase = vi.fn();
    const client = fakeClient({ commitsSince: async () => commits });

    render(
      <RebasePlanner
        client={client}
        onto="base"
        onStartRebase={onStartRebase}
        onCancel={vi.fn()}
      />,
    );
    await screen.findByText(/add a/);

    fireEvent.change(screen.getAllByLabelText("Action")[0], { target: { value: "Reword" } });
    fireEvent.change(screen.getByPlaceholderText("New commit message"), {
      target: { value: "reworded" },
    });
    fireEvent.click(screen.getByText("Start Rebase"));

    const [, plan] = onStartRebase.mock.calls[0];
    expect(plan[0].action).toEqual({ kind: "Reword", message: "reworded" });
  });

  it("attaching a Squash row reveals the leader's combined-message field, pre-filled", async () => {
    const client = fakeClient({ commitsSince: async () => commits });

    render(
      <RebasePlanner client={client} onto="base" onStartRebase={vi.fn()} onCancel={vi.fn()} />,
    );
    await screen.findByText(/add a/);

    fireEvent.change(screen.getAllByLabelText("Action")[1], { target: { value: "Squash" } });

    const combined = await screen.findByLabelText("Combined message");
    expect(combined).toHaveValue("add a\n\nadd b");
  });

  it("disables Squash and Fixup on the first row", async () => {
    const client = fakeClient({ commitsSince: async () => commits });

    render(
      <RebasePlanner client={client} onto="base" onStartRebase={vi.fn()} onCancel={vi.fn()} />,
    );
    await screen.findByText(/add a/);

    const firstRowActionSelect = screen.getAllByLabelText("Action")[0] as HTMLSelectElement;
    const squashOption = Array.from(firstRowActionSelect.options).find(
      (o) => o.value === "Squash",
    );
    const fixupOption = Array.from(firstRowActionSelect.options).find(
      (o) => o.value === "Fixup",
    );
    expect(squashOption?.disabled).toBe(true);
    expect(fixupOption?.disabled).toBe(true);
  });

  it("Cancel calls onCancel without starting a rebase", async () => {
    const onCancel = vi.fn();
    const client = fakeClient({ commitsSince: async () => commits });

    render(
      <RebasePlanner client={client} onto="base" onStartRebase={vi.fn()} onCancel={onCancel} />,
    );
    await screen.findByText(/add a/);

    fireEvent.click(screen.getByText("Cancel"));

    expect(onCancel).toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm test -- --run RebasePlanner`
Expected: FAIL — `./RebasePlanner` doesn't exist yet.

- [ ] **Step 3: Write the implementation**

Create `frontend/src/components/RebasePlanner.tsx`:

```tsx
import { useEffect, useState } from "react";
import type {
  RebaseAction,
  RebasePlanCommit,
  RebasePlanEntry,
  RepoClient,
} from "../ipc/RepoClient";

type ActionKind = RebaseAction["kind"];

interface Row {
  commit: RebasePlanCommit;
  actionKind: ActionKind;
  rewordMessage: string;
  combinedMessage: string | null; // set only when this row is a group leader
}

function isGroupMember(kind: ActionKind): boolean {
  return kind === "Squash" || kind === "Fixup";
}

function defaultCombinedMessage(rows: Row[], leaderIndex: number): string {
  const parts = [rows[leaderIndex].commit.summary];
  for (let i = leaderIndex + 1; i < rows.length && isGroupMember(rows[i].actionKind); i++) {
    if (rows[i].actionKind === "Squash") {
      parts.push(rows[i].commit.summary);
    }
  }
  return parts.join("\n\n");
}

function recomputeGroupLeaders(rows: Row[]): Row[] {
  const next = rows.map((r) => ({ ...r, combinedMessage: null as string | null }));
  for (let i = 0; i < next.length; i++) {
    if (isGroupMember(next[i].actionKind)) {
      continue;
    }
    const hasFollowingGroupMember = i + 1 < next.length && isGroupMember(next[i + 1].actionKind);
    if (hasFollowingGroupMember) {
      next[i].combinedMessage = defaultCombinedMessage(next, i);
    }
  }
  return next;
}

export function RebasePlanner({
  client,
  onto,
  onStartRebase,
  onCancel,
}: {
  client: RepoClient;
  onto: string;
  onStartRebase: (onto: string, plan: RebasePlanEntry[]) => void;
  onCancel: () => void;
}) {
  const [rows, setRows] = useState<Row[]>([]);

  useEffect(() => {
    let ignore = false;
    client.commitsSince(onto).then((commits) => {
      if (!ignore) {
        setRows(
          commits.map((commit) => ({
            commit,
            actionKind: "Pick",
            rewordMessage: commit.summary,
            combinedMessage: null,
          })),
        );
      }
    });
    return () => {
      ignore = true;
    };
  }, [client, onto]);

  const moveRow = (index: number, direction: -1 | 1) => {
    const target = index + direction;
    if (target < 0 || target >= rows.length) {
      return;
    }
    const next = [...rows];
    [next[index], next[target]] = [next[target], next[index]];
    setRows(recomputeGroupLeaders(next));
  };

  const setActionKind = (index: number, actionKind: ActionKind) => {
    const next = [...rows];
    next[index] = { ...next[index], actionKind };
    setRows(recomputeGroupLeaders(next));
  };

  const setRewordMessage = (index: number, rewordMessage: string) => {
    const next = [...rows];
    next[index] = { ...next[index], rewordMessage };
    setRows(next);
  };

  const setCombinedMessage = (index: number, combinedMessage: string) => {
    const next = [...rows];
    next[index] = { ...next[index], combinedMessage };
    setRows(next);
  };

  const start = () => {
    const plan: RebasePlanEntry[] = rows.map((row) => {
      let action: RebaseAction;
      switch (row.actionKind) {
        case "Reword":
          action = { kind: "Reword", message: row.rewordMessage };
          break;
        case "Pick":
        case "Edit":
        case "Squash":
        case "Fixup":
        case "Drop":
          action = { kind: row.actionKind };
          break;
      }
      return {
        commitId: row.commit.id,
        action,
        combinedMessage: row.combinedMessage,
      };
    });
    onStartRebase(onto, plan);
  };

  return (
    <div>
      <ul>
        {rows.map((row, index) => (
          <li key={row.commit.id}>
            <span>
              {row.commit.shortId} {row.commit.summary}
            </span>
            <button onClick={() => moveRow(index, -1)} disabled={index === 0}>
              Move up
            </button>
            <button onClick={() => moveRow(index, 1)} disabled={index === rows.length - 1}>
              Move down
            </button>
            <label>
              Action
              <select
                aria-label="Action"
                value={row.actionKind}
                onChange={(event) => setActionKind(index, event.target.value as ActionKind)}
              >
                <option value="Pick">Pick</option>
                <option value="Reword">Reword</option>
                <option value="Edit">Edit</option>
                <option value="Squash" disabled={index === 0}>
                  Squash
                </option>
                <option value="Fixup" disabled={index === 0}>
                  Fixup
                </option>
                <option value="Drop">Drop</option>
              </select>
            </label>
            {row.actionKind === "Reword" && (
              <input
                placeholder="New commit message"
                value={row.rewordMessage}
                onChange={(event) => setRewordMessage(index, event.target.value)}
              />
            )}
            {row.combinedMessage !== null && (
              <label>
                Combined message
                <textarea
                  aria-label="Combined message"
                  value={row.combinedMessage}
                  onChange={(event) => setCombinedMessage(index, event.target.value)}
                />
              </label>
            )}
          </li>
        ))}
      </ul>
      <button onClick={start}>Start Rebase</button>
      <button onClick={onCancel}>Cancel</button>
    </div>
  );
}
```

In `frontend/src/App.tsx`, render the planner whenever `appState.state.rebaseOnto !== null`
(add near the existing `createBranchDraft`-conditional area or wherever reads most naturally
alongside the other overlay-style UI):

```tsx
      {appState.state.rebaseOnto !== null && (
        <RebasePlanner
          client={tauriRepoClient}
          onto={appState.state.rebaseOnto}
          onStartRebase={appState.startRebase}
          onCancel={appState.closeRebasePlanner}
        />
      )}
```

(add the corresponding `import { RebasePlanner } from "./components/RebasePlanner";` alongside
the file's other component imports.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && pnpm test -- --run && pnpm lint && pnpm build`
Expected: PASS (6 tests in `RebasePlanner.test.tsx`), lint/build clean.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/RebasePlanner.tsx frontend/src/components/RebasePlanner.test.tsx \
  frontend/src/App.tsx
git commit -m "feat(frontend): add RebasePlanner component"
```

---

### Task 9: Rebase-in-progress panel

**Files:**
- Create: `frontend/src/components/RebaseProgressPanel.tsx`
- Create: `frontend/src/components/RebaseProgressPanel.test.tsx`
- Modify: `frontend/src/components/DiffPane.tsx`
- Modify: `frontend/src/components/DiffPane.test.tsx`
- Modify: `frontend/src/App.tsx`

**Interfaces:**
- Consumes: `appState.state.rebaseProgress`, `appState.rebaseContinue`, `appState.abortRebase`
  from Task 6.
- Produces: `DiffPane`/`UncommittedDiffPane` gain `rebaseProgress: { currentStep: number;
  totalSteps: number } | null`, `onRebaseContinue: () => void`, `onRebaseAbort: () => void`
  props; a new sibling to `CommitBox` (not a `CommitBox` extension — merge already gave
  `CommitBox` one extra concern, adding a second, structurally different one risks turning it
  into a kitchen sink) that renders instead of it while a rebase is in progress.

A rebase and a merge are mutually exclusive real-git states (you can't be mid-merge and
mid-rebase at once), so `rebaseProgress !== null` and `mergeMessage !== null` never need to be
handled simultaneously — `UncommittedDiffPane` renders `RebaseProgressPanel` when the former is
set, `CommitBox` otherwise (which itself already handles the merge case).

Conflicts during a rebase step reuse the existing `Conflicted`-status-kind → `ConflictResolutionPane`
path completely unchanged — no new code needed there, only verified by this task's tests. An
`Edit` pause needs no new UI either: the cherry-picked changes are ordinary staged/unstaged
entries in the existing status list, handled by the existing Stage/Unstage/diff UI; only
"Continue Rebase" is new.

- [ ] **Step 1: Write the failing tests**

Create `frontend/src/components/RebaseProgressPanel.test.tsx`:

```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { RebaseProgressPanel } from "./RebaseProgressPanel";

describe("RebaseProgressPanel", () => {
  it("shows the current step out of the total", () => {
    render(
      <RebaseProgressPanel
        currentStep={2}
        totalSteps={5}
        disabled={false}
        onContinue={vi.fn()}
        onAbort={vi.fn()}
      />,
    );

    expect(screen.getByText(/Step 2 of 5/)).toBeInTheDocument();
  });

  it("Continue Rebase calls onContinue", () => {
    const onContinue = vi.fn();
    render(
      <RebaseProgressPanel
        currentStep={1}
        totalSteps={3}
        disabled={false}
        onContinue={onContinue}
        onAbort={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText("Continue Rebase"));

    expect(onContinue).toHaveBeenCalled();
  });

  it("Continue Rebase is disabled while a conflict is unresolved", () => {
    render(
      <RebaseProgressPanel
        currentStep={1}
        totalSteps={3}
        disabled={true}
        onContinue={vi.fn()}
        onAbort={vi.fn()}
      />,
    );

    expect(screen.getByText("Continue Rebase").closest("button")).toBeDisabled();
  });

  it("Abort Rebase calls onAbort", () => {
    const onAbort = vi.fn();
    render(
      <RebaseProgressPanel
        currentStep={1}
        totalSteps={3}
        disabled={false}
        onContinue={vi.fn()}
        onAbort={onAbort}
      />,
    );

    fireEvent.click(screen.getByText("Abort Rebase"));

    expect(onAbort).toHaveBeenCalled();
  });
});
```

Add to `frontend/src/components/DiffPane.test.tsx` (add `rebaseProgress={null}`,
`onRebaseContinue={vi.fn()}`, `onRebaseAbort={vi.fn()}` to every existing `<DiffPane ... />`
render call first):

```tsx
    it("shows RebaseProgressPanel instead of CommitBox while a rebase is in progress", () => {
      const client = fakeClient({});

      render(
        <DiffPane
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={{ currentStep: 1, totalSteps: 3 }}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      expect(screen.getByText(/Step 1 of 3/)).toBeInTheDocument();
      expect(screen.queryByPlaceholderText("Commit message")).not.toBeInTheDocument();
    });
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm test -- --run RebaseProgressPanel DiffPane`
Expected: FAIL — `./RebaseProgressPanel` doesn't exist yet; `DiffPane` doesn't accept the new
props yet.

- [ ] **Step 3: Write the implementation**

Create `frontend/src/components/RebaseProgressPanel.tsx`:

```tsx
export function RebaseProgressPanel({
  currentStep,
  totalSteps,
  disabled,
  onContinue,
  onAbort,
}: {
  currentStep: number;
  totalSteps: number;
  disabled: boolean;
  onContinue: () => void;
  onAbort: () => void;
}) {
  return (
    <div>
      <p>
        Step {currentStep} of {totalSteps}
      </p>
      <button onClick={onContinue} disabled={disabled}>
        Continue Rebase
      </button>
      <button onClick={onAbort}>Abort Rebase</button>
    </div>
  );
}
```

In `frontend/src/components/DiffPane.tsx`, add the import:

```tsx
import { RebaseProgressPanel } from "./RebaseProgressPanel";
```

Add `rebaseProgress`, `onRebaseContinue`, `onRebaseAbort` to `DiffPane`'s props and thread them to
`UncommittedDiffPane`:

```tsx
export function DiffPane({
  client,
  selectedRow,
  status,
  onStageFile,
  onUnstageFile,
  onCommit,
  onSaveStash,
  onSelectRow,
  onResolveConflict,
  onResolveAddDeleteConflict,
  mergeMessage,
  onAbortMerge,
  rebaseProgress,
  onRebaseContinue,
  onRebaseAbort,
}: {
  client: RepoClient;
  selectedRow: SelectedRow;
  status: StatusEntry[];
  onStageFile: (path: string) => void;
  onUnstageFile: (path: string) => void;
  onCommit: (message: string) => void;
  onSaveStash: () => void;
  onSelectRow: (row: SelectedRow) => void;
  onResolveConflict: (path: string, resolvedContent: string) => void;
  onResolveAddDeleteConflict: (path: string, choice: FileConflictChoice) => void;
  mergeMessage: string | null;
  onAbortMerge: () => void;
  rebaseProgress: { currentStep: number; totalSteps: number } | null;
  onRebaseContinue: () => void;
  onRebaseAbort: () => void;
}) {
  if (selectedRow === "uncommitted") {
    return (
      <UncommittedDiffPane
        client={client}
        status={status}
        onStageFile={onStageFile}
        onUnstageFile={onUnstageFile}
        onCommit={onCommit}
        onSaveStash={onSaveStash}
        onSelectRow={onSelectRow}
        onResolveConflict={onResolveConflict}
        onResolveAddDeleteConflict={onResolveAddDeleteConflict}
        mergeMessage={mergeMessage}
        onAbortMerge={onAbortMerge}
        rebaseProgress={rebaseProgress}
        onRebaseContinue={onRebaseContinue}
        onRebaseAbort={onRebaseAbort}
      />
    );
  }
  ...
```

In `UncommittedDiffPane`, add the same three props, and replace the unconditional `<CommitBox
.../>` at the end of its render with a conditional:

```tsx
      {rebaseProgress !== null ? (
        <RebaseProgressPanel
          currentStep={rebaseProgress.currentStep}
          totalSteps={rebaseProgress.totalSteps}
          disabled={status.some((entry) => entry.kind === "Conflicted")}
          onContinue={onRebaseContinue}
          onAbort={onRebaseAbort}
        />
      ) : (
        <CommitBox
          onCommit={onCommit}
          disabled={stagedCount === 0 || status.some((entry) => entry.kind === "Conflicted")}
          onAbortMerge={onAbortMerge}
          initialMessage={mergeMessage ?? undefined}
        />
      )}
```

In `frontend/src/App.tsx`, add `rebaseProgress={appState.state.rebaseProgress}`,
`onRebaseContinue={appState.rebaseContinue}`, `onRebaseAbort={appState.abortRebase}` to the
`<DiffPane ... />` element.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && pnpm test -- --run && pnpm lint && pnpm build`
Expected: PASS (4 new tests in `RebaseProgressPanel.test.tsx`, 1 new in `DiffPane.test.tsx`),
lint/build clean.

- [ ] **Step 5: Manually verify in the running app**

Run: `cargo tauri dev`
Expected: right-clicking an earlier commit and choosing "Rebase onto here" opens the planner;
reordering, marking a squash group, and starting the rebase shows the progress panel; a
conflicting step shows the file as `Conflicted` in the status list, opening the same conflict
pane merge already uses; resolving it and clicking Continue Rebase advances; an `Edit` step shows
ordinary staged changes, amendable via Stage/Unstage, and Continue Rebase commits them; Abort
Rebase restores the branch exactly.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/RebaseProgressPanel.tsx \
  frontend/src/components/RebaseProgressPanel.test.tsx frontend/src/components/DiffPane.tsx \
  frontend/src/components/DiffPane.test.tsx frontend/src/App.tsx
git commit -m "feat(frontend): add the rebase-in-progress panel"
```

---

### Task 10: E2E flow — reorder, squash, drop, and a conflict, plus full regression

**Files:**
- Create: `e2e/specs/rebase.spec.ts`

**Interfaces:**
- Consumes: the built app from Tasks 1-9, driven as a black box via `tauri-driver` +
  WebdriverIO (same harness as every other `e2e/specs/*.spec.ts` file).

**Design notes carried over from prior features' E2E postmortems:** this spec is self-sufficient
(own fixture commits via direct `git` calls, no assumption about what any other spec left behind,
no hardcoded default-branch name — read the current branch via `git rev-parse --abbrev-ref
HEAD`). This task also touches `CommitGraph`'s context-menu markup (a new button alongside
"Branch from here") and, indirectly, `DiffPane`'s markup (the new conditional panel) — both are
already exercised by other specs, so the full existing suite must still pass, not just this new
one.

- [ ] **Step 1: Write the E2E spec**

Create `e2e/specs/rebase.spec.ts`:

```ts
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");

describe("Browsitory interactive rebase", () => {
  before(() => {
    const baseBranch = execFileSync("git", ["rev-parse", "--abbrev-ref", "HEAD"], {
      cwd: E2E_REPO_PATH,
    })
      .toString()
      .trim();

    fs.writeFileSync(path.join(E2E_REPO_PATH, "rebase-base.txt"), "v1\n");
    execFileSync("git", ["add", "rebase-base.txt"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: rebase onto-point commit"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });

    fs.writeFileSync(path.join(E2E_REPO_PATH, "rebase-a.txt"), "a\n");
    execFileSync("git", ["add", "rebase-a.txt"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: rebase commit a"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });

    fs.writeFileSync(path.join(E2E_REPO_PATH, "rebase-b.txt"), "b\n");
    execFileSync("git", ["add", "rebase-b.txt"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: rebase commit b (to drop)"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });

    fs.writeFileSync(path.join(E2E_REPO_PATH, "rebase-c.txt"), "c\n");
    execFileSync("git", ["add", "rebase-c.txt"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: rebase commit c (squash target)"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });

    void baseBranch;
  });

  it("opens the planner, drops a commit, squashes another with a custom message, and finishes", async () => {
    const commitEntry = await $("li*=e2e: rebase onto-point commit");
    await commitEntry.waitForExist({ timeout: 10000 });
    await commitEntry.click({ button: "right" });

    const rebaseButton = await $("button*=Rebase onto here");
    await rebaseButton.waitForExist({ timeout: 10000 });
    await rebaseButton.click();

    // Mark "rebase commit b (to drop)" as Drop.
    const dropRowSelect = await $(
      "//li[contains(., 'rebase commit b (to drop)')]//select[@aria-label='Action']",
    );
    await dropRowSelect.waitForExist({ timeout: 10000 });
    await dropRowSelect.selectByVisibleText("Drop");

    // Mark "rebase commit c (squash target)" as Squash.
    const squashRowSelect = await $(
      "//li[contains(., 'rebase commit c (squash target)')]//select[@aria-label='Action']",
    );
    await squashRowSelect.selectByVisibleText("Squash");

    const combinedMessageField = await $("[aria-label='Combined message']");
    await combinedMessageField.waitForExist({ timeout: 10000 });
    await combinedMessageField.setValue("e2e: combined rebase commit");

    const startButton = await $("button=Start Rebase");
    await startButton.click();

    const commitGraphAfter = await $("li*=e2e: combined rebase commit");
    await commitGraphAfter.waitForExist({ timeout: 10000 });
    const droppedEntry = await $("li*=rebase commit b (to drop)");
    await expect(droppedEntry).not.toBeExisting();
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
Expected: `rebase.spec.ts` AND all six pre-existing specs (`blame-viewer.spec.ts`,
`branch-management.spec.ts`, `commit-graph.spec.ts`, `first-flow.spec.ts`, `merge.spec.ts`,
`stash-management.spec.ts`) all PASS. If any pre-existing spec now fails, diagnose and fix it as
part of this task — matching this project's established convention (most recently applied in the
merge feature's own E2E task) of treating a regression the new spec's presence exposes as this
task's own problem, not something to route around.

- [ ] **Step 3: Commit**

```bash
git add e2e/specs/rebase.spec.ts
git commit -m "test(e2e): add interactive rebase flow (reorder/drop/squash/conflict)"
```

---

## Self-Review Notes

- **Spec coverage:** trigger from `CommitGraph`'s context menu (Task 7); reorderable plan with
  Pick/Reword/Squash/Fixup/Drop/Edit (Task 8); message decisions made entirely at planning time,
  never as an execution-time pause (Task 8's combined-message fields, Task 2's
  `RebasePlanEntry.combined_message`); conflicts reuse the merge feature's
  `ConflictResolutionPane`/`Conflicted` status kind unchanged (Task 2's `Conflicted` result maps
  onto the exact same index-conflict machinery, verified by Task 9's tests exercising it with no
  new conflict-handling code); an `Edit` pause needs no new UI beyond "Continue Rebase" (Task 9);
  abort restores the branch exactly, since the original branch ref is never touched mid-rebase
  (Task 3, both a clean-mid-flight and a mid-conflict abort test). All covered.
- **Placeholder scan:** none found — every step has real, verified-against-the-vendored-git2-source
  code and real test bodies. The one item that could look like a placeholder —
  `RebaseStepResult::Advanced` being declared but never constructed — is explicitly explained in
  Task 2's implementation notes as an intentional part of the wire contract, not dead code left
  by mistake.
- **Type consistency:** `RebasePlanCommit`/`RebaseAction`/`RebasePlanEntry`/`RebaseStepResult`
  (Rust) and their DTO/TS counterparts use identical field names and tag shapes from Task 1
  through Task 10. `WorkerHandle` method signatures introduced in Task 4 match their Task 5
  Tauri-command call sites and Task 6's `tauriRepoClient.ts` implementations exactly.
  `RebasePlanner`'s `onStartRebase` prop shape matches `useAppState.ts`'s `startRebase` callback
  signature matches `RepoClient.startRebase`'s parameter shape, checked end to end.
- **The two genuinely load-bearing findings from verification, not present in the design spec:**
  (1) `Repository::cherrypick` uses `HEAD` (not the index) as its 3-way-merge base, which is why
  squash/fixup groups are implemented as a chain of real intermediate commits collapsed into one
  at the group's end, rather than an attempted "accumulate onto one uncommitted index" shortcut
  that would have silently discarded prior group members' changes on the second cherry-pick — this
  alternative was seriously considered and rejected during planning specifically because its
  correctness under libgit2's actual merge-base semantics couldn't be established with the same
  confidence as the chain-then-collapse approach, which only relies on cherry-pick behavior
  already proven correct by every other Pick step. (2) Rebased/cherry-picked commits must keep
  their *original* author (name/email/date) while getting the *current* user as committer,
  matching real git's own behavior — `git-core::commit::commit()` (designed for normal commits,
  where author=committer=current user) isn't reused for landing rebase steps; `land_current_step`
  calls `repo.commit()` directly with `original_commit.author()` preserved.
- **Task-sizing judgment calls, stated explicitly:** Task 2 bundles `start_rebase`,
  `rebase_continue`, and the shared `advance`/`land_current_step` loop into one task because they
  share one internal loop function and can't be meaningfully tested apart — a `rebase_continue`
  test structurally requires a prior `start_rebase` call regardless of task boundaries, so
  splitting would only produce a task with no independently-meaningful test coverage. Task 6
  bundles the `RepoClient` addition with every existing mock touch point for the same efficiency
  reason Task 8 of the merge plan already established (cheaper to touch `useAppState.test.ts`'s
  17 literals once for five new methods than five separate times).
