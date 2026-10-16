# Feature Parity Batch 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add amend, cherry-pick, revert, reset (soft/mixed/hard), force push with lease, and detached-HEAD checkout, end to end (git-core → repo-service → Tauri/sidecar → `RepoClient` → UI).

**Architecture:** Native `git2` functions in `git-core` (new `pick.rs`, `reset.rs`; additions to `commit.rs`, `branch.rs`, `remote.rs`, `repo.rs`), exposed as worker commands in `repo-service`, then Tauri commands and sidecar JSON-RPC methods, then seven new `RepoClient` methods. Cherry-pick/revert conflicts reuse the merge flow: `MERGE_MSG` present ⇒ existing conflict pane, CommitBox prefill and `abortMerge` all work unchanged. A new `getHeadInfo` refines labels (operation kind, detached HEAD, HEAD id, pushed).

**Tech Stack:** Rust (`git2` 0.21, `thiserror`, `tempfile`), Tauri 2, React + TypeScript, Vitest + Testing Library, WebdriverIO (Tauri E2E), `@vscode/test-electron`.

**Spec:** `docs/superpowers/specs/2026-10-15-parity-batch1-design.md`

## Global Constraints

- Branch: `feat/parity-batch1`. Commit after every task. Commit messages end with the session attribution lines from the system reminder.
- `git-core`/`repo-service` tests use real temp-dir repos (`tests/common/mod.rs`: `init_repo`, `commit_all`, `write_file`), never a mocked `Repository`.
- Frontend tests mock `RepoClient` only — never `@tauri-apps/api` or `postMessage`. The transport-isolation ESLint rule must stay green (`pnpm lint`).
- No new dependencies.
- Backend error text is user-facing (it travels as `error.to_string()` into `state.error`). Use the spec's exact strings in `#[error(...)]`.
- Error strings (verbatim from spec):
  - `OperationInProgress`: "Finish or abort the current merge/cherry-pick/revert/rebase first."
  - `DirtyIndex`: "Commit or unstage your staged changes first."
  - `MergeCommitUnsupported`: "Cherry-picking or reverting merge commits isn't supported yet."
  - `NothingToAmend`: "There is no commit to amend yet."
  - `EmptyResult`: "Nothing to apply — those changes are already on this branch."
  - `StaleLease` (frontend message): "⟨remote⟩/⟨branch⟩ changed since your last fetch. Fetch and review before force pushing."
- Revert message format: `Revert "<summary>"\n\nThis reverts commit <full sha>.\n`
- UI labels are sentence case (existing FB-007 rule).
- Every PR touching `crates/`, `frontend/src/`, `e2e/` needs a `CHANGELOG.md` entry (pre-push hook) — Task 15.
- Commands (from `CLAUDE.md`): `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`; `cd frontend && pnpm lint && pnpm test -- --run`.

## Review Focus

1. **Cherry-pick/revert with unstaged edits to the same file** — must refuse and leave the user's file byte-for-byte intact (no silent overwrite). Test in Task 3.
2. **Cherry-picking an already-applied commit** — must not create an empty commit or leave `CHERRY_PICK_HEAD` behind; returns `EmptyResult`, repo state `Clean`. Test in Task 3.
3. **Hard reset with untracked files** — untracked files survive (git semantics); only tracked changes are discarded. Test in Task 4.
4. **Force push when the branch was never on the remote / remote has it but no local tracking ref** — first must succeed (zero-OID lease), second must refuse with `StaleLease`. Tests in Task 5.
5. **Amend toggle with a half-typed draft** — toggling on then off restores the draft exactly; toggling on never discards it. Test in Task 11.

---

## File Map

| File | Responsibility |
|---|---|
| `crates/git-core/src/repo.rs` (modify) | `HeadInfo`, `OperationKind`, `head_info` |
| `crates/git-core/src/commit.rs` (modify) | `amend_head`; `commit()` cleans `CherryPick`/`Revert` state |
| `crates/git-core/src/pick.rs` (create) | `PickOutcome`, `PickError`, `cherry_pick`, `revert` |
| `crates/git-core/src/reset.rs` (create) | `ResetMode`, `ResetError`, `reset_to` |
| `crates/git-core/src/branch.rs` (modify) | `checkout_commit` |
| `crates/git-core/src/remote.rs` (modify) | `force_push_current_branch`, `StaleLease`, `ForcePushBranch` |
| `crates/git-core/tests/{head_info,amend,pick,reset,checkout_commit}.rs` (create), `tests/remote.rs` (modify) | real-repo tests |
| `crates/repo-service/src/worker/history.rs` (create) | worker handlers + `WorkerHandle` methods for the non-transfer ops |
| `crates/repo-service/src/worker/{mod.rs,remote.rs}` (modify) | `Command` variants, dispatch, force-push transfer op |
| `crates/tauri-app/src/commands/history.rs` (create), `commands/{mod.rs,remote.rs}`, `main.rs` (modify) | Tauri commands + DTOs |
| `crates/vscode-sidecar/src/dispatch.rs` (modify), `tests/protocol_roundtrip.rs` (modify) | JSON-RPC methods |
| `frontend/src/ipc/{RepoClient.ts,tauriRepoClient.ts,vscodeRepoClient.ts}` (modify) | client surface |
| `frontend/src/state/useHistoryActions.ts` (create) | cherry-pick/revert/reset/checkout/amend/force-push actions |
| `frontend/src/state/{useAppState.ts,useRemoteTransferActions.ts}` (modify) | `headInfo` in state; force-push offer |
| `frontend/src/components/ResetDialog.tsx` (+ `.module.css`, `.test.tsx`) (create) | reset mode picker |
| `frontend/src/components/{CommitGraph,CommitBox,DiffPane,OperationStatusStrip,SyncBar}.tsx`, `App.tsx` (modify) | UI |
| `frontend/src/lib/commands.ts` (modify) | palette commands |
| `e2e/specs/history-rewrite.spec.ts` (create), `extension/e2e/src/specs/cherry-pick.spec.ts` (create) | E2E |
| `docs/USER_GUIDE.md`, `CHANGELOG.md` (modify) | docs |

---

### Task 1: `head_info` in git-core

**Files:**
- Modify: `crates/git-core/src/repo.rs`
- Test: `crates/git-core/tests/head_info.rs` (create)

**Interfaces:**
- Produces:
  ```rust
  pub enum OperationKind { Merge, CherryPick, Revert }          // Debug, Clone, Copy, PartialEq, Eq
  pub struct HeadInfo {                                          // Debug, Clone, PartialEq, Eq
      pub head_id: Option<String>,      // full sha, None on unborn HEAD
      pub detached_at: Option<String>,  // 7-char short sha when detached
      pub head_pushed: bool,
      pub operation: Option<OperationKind>,
  }
  pub fn head_info(repo: &git2::Repository) -> Result<HeadInfo, RepoError>
  ```

- [ ] **Step 1: Write the failing tests**

```rust
// crates/git-core/tests/head_info.rs
mod common;

use git_core::repo::{head_info, OperationKind};

#[test]
fn unborn_head_has_no_id_and_no_operation() {
    let (_dir, repo) = common::init_repo();
    let info = head_info(&repo).unwrap();
    assert_eq!(info.head_id, None);
    assert_eq!(info.detached_at, None);
    assert!(!info.head_pushed);
    assert_eq!(info.operation, None);
}

#[test]
fn attached_head_reports_full_id_and_not_detached() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a\n");
    common::commit_all(&repo, "first");
    let head = repo.head().unwrap().target().unwrap().to_string();
    let info = head_info(&repo).unwrap();
    assert_eq!(info.head_id.as_deref(), Some(head.as_str()));
    assert_eq!(info.detached_at, None);
}

#[test]
fn detached_head_reports_short_id() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a\n");
    common::commit_all(&repo, "first");
    let oid = repo.head().unwrap().target().unwrap();
    repo.set_head_detached(oid).unwrap();
    let info = head_info(&repo).unwrap();
    assert_eq!(info.detached_at, Some(oid.to_string()[..7].to_string()));
}

#[test]
fn head_pushed_is_true_only_when_upstream_contains_head() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a\n");
    common::commit_all(&repo, "first");
    let branch = repo.head().unwrap().shorthand().unwrap().to_string();
    let oid = repo.head().unwrap().target().unwrap();
    // Fake a fetched upstream at HEAD.
    repo.reference(&format!("refs/remotes/origin/{branch}"), oid, true, "test").unwrap();
    repo.remote("origin", "https://example.invalid/repo.git").unwrap();
    let mut config = repo.config().unwrap();
    config.set_str(&format!("branch.{branch}.remote"), "origin").unwrap();
    config.set_str(&format!("branch.{branch}.merge"), &format!("refs/heads/{branch}")).unwrap();
    assert!(head_info(&repo).unwrap().head_pushed);

    common::write_file(dir.path(), "b.txt", "b\n");
    common::commit_all(&repo, "unpushed");
    assert!(!head_info(&repo).unwrap().head_pushed);
}

#[test]
fn reports_cherry_pick_in_progress() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a\n");
    common::commit_all(&repo, "first");
    let oid = repo.head().unwrap().target().unwrap();
    std::fs::write(repo.path().join("CHERRY_PICK_HEAD"), format!("{oid}\n")).unwrap();
    assert_eq!(head_info(&repo).unwrap().operation, Some(OperationKind::CherryPick));
}

#[test]
fn reports_revert_and_merge_in_progress() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a\n");
    common::commit_all(&repo, "first");
    let oid = repo.head().unwrap().target().unwrap();
    std::fs::write(repo.path().join("REVERT_HEAD"), format!("{oid}\n")).unwrap();
    assert_eq!(head_info(&repo).unwrap().operation, Some(OperationKind::Revert));
    std::fs::remove_file(repo.path().join("REVERT_HEAD")).unwrap();
    std::fs::write(repo.path().join("MERGE_HEAD"), format!("{oid}\n")).unwrap();
    assert_eq!(head_info(&repo).unwrap().operation, Some(OperationKind::Merge));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p git-core --test head_info`
Expected: compile error, `head_info` / `OperationKind` not found in `git_core::repo`.

- [ ] **Step 3: Implement**

Append to `crates/git-core/src/repo.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationKind {
    Merge,
    CherryPick,
    Revert,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadInfo {
    pub head_id: Option<String>,
    pub detached_at: Option<String>,
    pub head_pushed: bool,
    pub operation: Option<OperationKind>,
}

/// Snapshot of what HEAD is and which single-commit operation (if any) is paused on it. Rebase
/// is deliberately absent — it has its own richer `rebase::rebase_progress`.
pub fn head_info(repo: &git2::Repository) -> Result<HeadInfo, RepoError> {
    use git2::RepositoryState;
    let operation = match repo.state() {
        RepositoryState::Merge => Some(OperationKind::Merge),
        RepositoryState::CherryPick | RepositoryState::CherryPickSequence => {
            Some(OperationKind::CherryPick)
        }
        RepositoryState::Revert | RepositoryState::RevertSequence => Some(OperationKind::Revert),
        _ => None,
    };
    let head = match repo.head() {
        Ok(head) => head,
        Err(error) if error.code() == git2::ErrorCode::UnbornBranch => {
            return Ok(HeadInfo { head_id: None, detached_at: None, head_pushed: false, operation });
        }
        Err(error) => return Err(error.into()),
    };
    let head_oid = head.peel_to_commit()?.id();
    let detached = repo.head_detached()?;
    let detached_at = detached.then(|| head_oid.to_string()[..7].to_string());
    let head_pushed = !detached && upstream_contains(repo, &head, head_oid);
    Ok(HeadInfo { head_id: Some(head_oid.to_string()), detached_at, head_pushed, operation })
}

fn upstream_contains(repo: &git2::Repository, head: &git2::Reference<'_>, head_oid: git2::Oid) -> bool {
    let Some(name) = head.shorthand() else { return false };
    let Ok(branch) = repo.find_branch(name, git2::BranchType::Local) else { return false };
    let Ok(upstream) = branch.upstream() else { return false };
    let Some(upstream_oid) = upstream.get().target() else { return false };
    upstream_oid == head_oid || repo.graph_descendant_of(upstream_oid, head_oid).unwrap_or(false)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p git-core --test head_info`
Expected: 6 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/git-core/src/repo.rs crates/git-core/tests/head_info.rs
git commit -m "feat(git-core): head_info reports HEAD id, detached state, pushed state and paused operation"
```

---

### Task 2: Amend + `commit()` finishing cherry-pick/revert

**Files:**
- Modify: `crates/git-core/src/commit.rs`
- Test: `crates/git-core/tests/amend.rs` (create)

**Interfaces:**
- Produces:
  ```rust
  pub enum CommitError { Write(git2::Error), NothingToAmend, OperationInProgress }
  pub fn amend_head(repo: &git2::Repository, message: &str) -> Result<String, CommitError> // new HEAD sha
  ```
- `commit()` behavior change: when `repo.state()` is `CherryPick`/`Revert`, parents stay HEAD-only and `cleanup_state()` runs after writing.

- [ ] **Step 1: Write the failing tests**

```rust
// crates/git-core/tests/amend.rs
mod common;

use git_core::commit::{amend_head, commit, CommitError};

fn repo_with_two_commits() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a\n");
    common::commit_all(&repo, "first");
    common::write_file(dir.path(), "b.txt", "b\n");
    common::commit_all(&repo, "second");
    (dir, repo)
}

#[test]
fn amend_folds_staged_changes_and_new_message_into_head() {
    let (dir, repo) = repo_with_two_commits();
    let old_head = repo.head().unwrap().peel_to_commit().unwrap();
    let parent_id = old_head.parent_id(0).unwrap();
    common::write_file(dir.path(), "c.txt", "c\n");
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("c.txt")).unwrap();
    index.write().unwrap();

    let new_id = amend_head(&repo, "second, amended").unwrap();

    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.id().to_string(), new_id);
    assert_ne!(head.id(), old_head.id());
    assert_eq!(head.message(), Some("second, amended"));
    assert_eq!(head.parent_ids().collect::<Vec<_>>(), vec![parent_id]);
    assert_eq!(head.author().name(), old_head.author().name());
    assert!(head.tree().unwrap().get_name("c.txt").is_some());
    assert!(repo.head().unwrap().is_branch(), "branch ref moved, HEAD still attached");
}

#[test]
fn message_only_amend_keeps_tree() {
    let (_dir, repo) = repo_with_two_commits();
    let old_tree = repo.head().unwrap().peel_to_commit().unwrap().tree_id();
    amend_head(&repo, "reworded").unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.tree_id(), old_tree);
    assert_eq!(head.message(), Some("reworded"));
}

#[test]
fn amend_works_on_detached_head() {
    let (_dir, repo) = repo_with_two_commits();
    let oid = repo.head().unwrap().target().unwrap();
    repo.set_head_detached(oid).unwrap();
    let new_id = amend_head(&repo, "detached amend").unwrap();
    assert_eq!(repo.head().unwrap().target().unwrap().to_string(), new_id);
    assert!(repo.head_detached().unwrap());
}

#[test]
fn amend_refuses_unborn_head() {
    let (_dir, repo) = common::init_repo();
    assert!(matches!(amend_head(&repo, "x"), Err(CommitError::NothingToAmend)));
}

#[test]
fn amend_refuses_during_a_merge() {
    let (_dir, repo) = repo_with_two_commits();
    let oid = repo.head().unwrap().target().unwrap();
    std::fs::write(repo.path().join("MERGE_HEAD"), format!("{oid}\n")).unwrap();
    assert!(matches!(amend_head(&repo, "x"), Err(CommitError::OperationInProgress)));
}

#[test]
fn commit_finishes_a_paused_cherry_pick_with_a_single_parent_and_clean_state() {
    let (dir, mut repo) = repo_with_two_commits();
    let head = repo.head().unwrap().target().unwrap();
    std::fs::write(repo.path().join("CHERRY_PICK_HEAD"), format!("{head}\n")).unwrap();
    common::write_file(dir.path(), "d.txt", "d\n");
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("d.txt")).unwrap();
    index.write().unwrap();

    commit(&mut repo, "picked").unwrap();

    let new_head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(new_head.parent_count(), 1);
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p git-core --test amend`
Expected: compile error (`amend_head`, `NothingToAmend`, `OperationInProgress` missing).

- [ ] **Step 3: Implement**

In `crates/git-core/src/commit.rs`, extend the enum:

```rust
#[derive(Debug, Error)]
pub enum CommitError {
    #[error("failed to create commit: {0}")]
    Write(#[from] git2::Error),
    #[error("There is no commit to amend yet.")]
    NothingToAmend,
    #[error("Finish or abort the current merge/cherry-pick/revert/rebase first.")]
    OperationInProgress,
}
```

At the end of `commit()`, replace the `if is_merging { repo.cleanup_state()?; }` block with:

```rust
    // Cherry-pick/revert leave CHERRY_PICK_HEAD/REVERT_HEAD + MERGE_MSG behind on conflict; the
    // commit that lands the resolved result is single-parent (HEAD only — they are not merges),
    // but the state files must still be cleared or `repo.state()` stays non-Clean.
    let finishes_pick = matches!(
        repo.state(),
        git2::RepositoryState::CherryPick | git2::RepositoryState::Revert
    );
    if is_merging || finishes_pick {
        repo.cleanup_state()?;
    }
```

(Keep the existing return of `oid.to_string()` that follows — read the function tail before editing and preserve it.)

Add:

```rust
/// Rewrites HEAD's commit with the current index tree and `message`, keeping its parents and
/// author; committer becomes `repo.signature()`. Works on a branch (the branch ref moves) or a
/// detached HEAD. Refused while any operation (merge, cherry-pick, revert, rebase) is paused,
/// since amending then would silently drop that operation's extra state.
pub fn amend_head(repo: &git2::Repository, message: &str) -> Result<String, CommitError> {
    if repo.state() != git2::RepositoryState::Clean {
        return Err(CommitError::OperationInProgress);
    }
    let head = match repo.head() {
        Ok(head) => head.peel_to_commit()?,
        Err(error) if error.code() == git2::ErrorCode::UnbornBranch => {
            return Err(CommitError::NothingToAmend)
        }
        Err(error) => return Err(error.into()),
    };
    let tree_id = repo.index()?.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let committer = repo.signature()?;
    let oid = head.amend(Some("HEAD"), None, Some(&committer), None, Some(message), Some(&tree))?;
    Ok(oid.to_string())
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p git-core --test amend && cargo test -p git-core --test stage_commit --test merge`
Expected: all pass (existing merge/commit tests unaffected).

- [ ] **Step 5: Commit**

```bash
git add crates/git-core/src/commit.rs crates/git-core/tests/amend.rs
git commit -m "feat(git-core): amend_head; commit() finishes a paused cherry-pick or revert"
```

---

### Task 3: Cherry-pick and revert

**Files:**
- Create: `crates/git-core/src/pick.rs`
- Modify: `crates/git-core/src/lib.rs` (add `pub mod pick;`)
- Test: `crates/git-core/tests/pick.rs` (create)

**Interfaces:**
- Consumes: `git_core::merge::abort_merge` (tests only).
- Produces:
  ```rust
  pub enum PickOutcome { Committed { commit_id: String }, Conflicted { files: Vec<String> } }
  pub enum PickError { Git(git2::Error), OperationInProgress, DirtyIndex, MergeCommitUnsupported, UnbornHead, EmptyResult }
  pub fn cherry_pick(repo: &git2::Repository, commit_id: &str) -> Result<PickOutcome, PickError>
  pub fn revert(repo: &git2::Repository, commit_id: &str) -> Result<PickOutcome, PickError>
  ```

- [ ] **Step 1: Write the failing tests**

```rust
// crates/git-core/tests/pick.rs
mod common;

use git_core::merge::abort_merge;
use git_core::pick::{cherry_pick, revert, PickError, PickOutcome};
use std::path::Path;

/// main: base(shared.txt="one\n") ; feature: +feature.txt ; main stays at base.
fn repo_with_feature_branch() -> (tempfile::TempDir, git2::Repository, String) {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "shared.txt", "one\n");
    common::commit_all(&repo, "base");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &base, false).unwrap();
    repo.set_head("refs/heads/feature").unwrap();
    common::write_file(dir.path(), "feature.txt", "feature\n");
    common::commit_all(&repo, "add feature");
    let feature_tip = repo.head().unwrap().target().unwrap().to_string();
    let main = repo.find_branch("master", git2::BranchType::Local)
        .or_else(|_| repo.find_branch("main", git2::BranchType::Local)).unwrap();
    let main_ref = main.get().name().unwrap().to_string();
    repo.set_head(&main_ref).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();
    (dir, repo, feature_tip)
}

fn stage(repo: &git2::Repository, path: &str) {
    let mut index = repo.index().unwrap();
    index.add_path(Path::new(path)).unwrap();
    index.write().unwrap();
}

#[test]
fn clean_cherry_pick_commits_with_original_message_and_author() {
    let (dir, repo, feature_tip) = repo_with_feature_branch();
    let outcome = cherry_pick(&repo, &feature_tip).unwrap();
    let PickOutcome::Committed { commit_id } = outcome else { panic!("expected commit") };
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.id().to_string(), commit_id);
    assert_ne!(commit_id, feature_tip);
    assert_eq!(head.message(), Some("add feature"));
    assert_eq!(head.parent_count(), 1);
    assert!(dir.path().join("feature.txt").exists());
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn conflicting_cherry_pick_pauses_with_merge_msg_and_can_be_finished() {
    let (dir, repo, _) = repo_with_feature_branch();
    // feature2 edits shared.txt one way, main another.
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature2", &base, false).unwrap();
    repo.set_head("refs/heads/feature2").unwrap();
    common::write_file(dir.path(), "shared.txt", "theirs\n");
    common::commit_all(&repo, "theirs edit");
    let theirs = repo.head().unwrap().target().unwrap().to_string();
    repo.set_head(base_branch_ref(&repo).as_str()).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();
    common::write_file(dir.path(), "shared.txt", "ours\n");
    common::commit_all(&repo, "ours edit");

    let outcome = cherry_pick(&repo, &theirs).unwrap();

    assert_eq!(outcome, PickOutcome::Conflicted { files: vec!["shared.txt".into()] });
    assert_eq!(repo.state(), git2::RepositoryState::CherryPick);
    assert!(git_core::merge::merge_message(&repo).unwrap().contains("theirs edit"));

    common::write_file(dir.path(), "shared.txt", "resolved\n");
    stage(&repo, "shared.txt");
    let mut repo = repo;
    git_core::commit::commit(&mut repo, "theirs edit").unwrap();
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn aborting_a_conflicted_cherry_pick_restores_head_and_files() {
    let (dir, repo, _) = repo_with_feature_branch();
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature2", &base, false).unwrap();
    repo.set_head("refs/heads/feature2").unwrap();
    common::write_file(dir.path(), "shared.txt", "theirs\n");
    common::commit_all(&repo, "theirs edit");
    let theirs = repo.head().unwrap().target().unwrap().to_string();
    repo.set_head(base_branch_ref(&repo).as_str()).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();
    common::write_file(dir.path(), "shared.txt", "ours\n");
    common::commit_all(&repo, "ours edit");
    let before = repo.head().unwrap().target().unwrap();

    cherry_pick(&repo, &theirs).unwrap();
    abort_merge(&repo).unwrap();

    assert_eq!(repo.head().unwrap().target().unwrap(), before);
    assert_eq!(std::fs::read_to_string(dir.path().join("shared.txt")).unwrap(), "ours\n");
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn cherry_picking_an_already_applied_commit_is_empty_and_leaves_state_clean() {
    let (_dir, repo, feature_tip) = repo_with_feature_branch();
    cherry_pick(&repo, &feature_tip).unwrap();
    let head_before = repo.head().unwrap().target().unwrap();
    assert!(matches!(cherry_pick(&repo, &feature_tip), Err(PickError::EmptyResult)));
    assert_eq!(repo.head().unwrap().target().unwrap(), head_before);
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn cherry_pick_refuses_staged_changes() {
    let (dir, repo, feature_tip) = repo_with_feature_branch();
    common::write_file(dir.path(), "staged.txt", "x\n");
    stage(&repo, "staged.txt");
    assert!(matches!(cherry_pick(&repo, &feature_tip), Err(PickError::DirtyIndex)));
}

#[test]
fn cherry_pick_never_overwrites_an_unstaged_edit_to_a_touched_file() {
    let (dir, repo, _) = repo_with_feature_branch();
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("edit", &base, false).unwrap();
    repo.set_head("refs/heads/edit").unwrap();
    common::write_file(dir.path(), "shared.txt", "from edit branch\n");
    common::commit_all(&repo, "edit shared");
    let edit = repo.head().unwrap().target().unwrap().to_string();
    repo.set_head(base_branch_ref(&repo).as_str()).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();
    common::write_file(dir.path(), "shared.txt", "my unsaved work\n");

    assert!(cherry_pick(&repo, &edit).is_err());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("shared.txt")).unwrap(),
        "my unsaved work\n"
    );
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn cherry_pick_refuses_merge_commits_and_paused_operations() {
    let (dir, repo, feature_tip) = repo_with_feature_branch();
    // Build a merge commit by hand.
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let other = repo.find_commit(git2::Oid::from_str(&feature_tip).unwrap()).unwrap();
    let sig = repo.signature().unwrap();
    let tree = other.tree().unwrap();
    let merge_id = repo.commit(None, &sig, &sig, "merge", &tree, &[&head, &other]).unwrap();
    assert!(matches!(
        cherry_pick(&repo, &merge_id.to_string()),
        Err(PickError::MergeCommitUnsupported)
    ));
    assert!(matches!(revert(&repo, &merge_id.to_string()), Err(PickError::MergeCommitUnsupported)));

    std::fs::write(repo.path().join("MERGE_HEAD"), format!("{}\n", head.id())).unwrap();
    assert!(matches!(cherry_pick(&repo, &feature_tip), Err(PickError::OperationInProgress)));
    let _ = dir;
}

#[test]
fn clean_revert_commits_the_inverse_with_the_standard_message() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "b.txt", "b\n");
    common::commit_all(&repo, "add b");
    let target = repo.head().unwrap().target().unwrap();

    let PickOutcome::Committed { .. } = revert(&repo, &target.to_string()).unwrap() else {
        panic!("expected commit")
    };

    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(
        head.message().unwrap(),
        format!("Revert \"add b\"\n\nThis reverts commit {target}.\n")
    );
    assert!(!dir.path().join("b.txt").exists());
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn conflicting_revert_pauses_in_revert_state() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "one\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "a.txt", "two\n");
    common::commit_all(&repo, "to two");
    let target = repo.head().unwrap().target().unwrap();
    common::write_file(dir.path(), "a.txt", "three\n");
    common::commit_all(&repo, "to three");

    let outcome = revert(&repo, &target.to_string()).unwrap();

    assert_eq!(outcome, PickOutcome::Conflicted { files: vec!["a.txt".into()] });
    assert_eq!(repo.state(), git2::RepositoryState::Revert);
}

fn base_branch_ref(repo: &git2::Repository) -> String {
    for name in ["main", "master"] {
        if let Ok(branch) = repo.find_branch(name, git2::BranchType::Local) {
            return branch.get().name().unwrap().to_string();
        }
    }
    panic!("no base branch");
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p git-core --test pick`
Expected: compile error, unresolved module `git_core::pick`.

- [ ] **Step 3: Implement**

Add `pub mod pick;` to `crates/git-core/src/lib.rs` (alphabetical, after `merge`). Create `crates/git-core/src/pick.rs`:

```rust
use git2::{Commit, Oid, Repository, RepositoryState};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickOutcome {
    Committed { commit_id: String },
    Conflicted { files: Vec<String> },
}

#[derive(Debug, Error)]
pub enum PickError {
    #[error("{0}")]
    Git(#[from] git2::Error),
    #[error("Finish or abort the current merge/cherry-pick/revert/rebase first.")]
    OperationInProgress,
    #[error("Commit or unstage your staged changes first.")]
    DirtyIndex,
    #[error("Cherry-picking or reverting merge commits isn't supported yet.")]
    MergeCommitUnsupported,
    #[error("There is no commit on this branch yet.")]
    UnbornHead,
    #[error("Nothing to apply — those changes are already on this branch.")]
    EmptyResult,
}

#[derive(Clone, Copy)]
enum Kind {
    CherryPick,
    Revert,
}

/// Applies `commit_id` on top of HEAD. A clean result is committed immediately with the original
/// message and author; a conflict leaves the repo in `RepositoryState::CherryPick` with
/// `MERGE_MSG` written, for the existing conflict pane + `commit::commit` / `merge::abort_merge`
/// to finish.
pub fn cherry_pick(repo: &Repository, commit_id: &str) -> Result<PickOutcome, PickError> {
    apply(repo, commit_id, Kind::CherryPick)
}

/// Commits the inverse of `commit_id` on top of HEAD. Same conflict contract as `cherry_pick`
/// (state `RepositoryState::Revert`).
pub fn revert(repo: &Repository, commit_id: &str) -> Result<PickOutcome, PickError> {
    apply(repo, commit_id, Kind::Revert)
}

fn apply(repo: &Repository, commit_id: &str, kind: Kind) -> Result<PickOutcome, PickError> {
    if repo.state() != RepositoryState::Clean {
        return Err(PickError::OperationInProgress);
    }
    let head = match repo.head() {
        Ok(head) => head.peel_to_commit()?,
        Err(error) if error.code() == git2::ErrorCode::UnbornBranch => {
            return Err(PickError::UnbornHead)
        }
        Err(error) => return Err(error.into()),
    };
    let target = repo.find_commit(Oid::from_str(commit_id)?)?;
    if target.parent_count() > 1 {
        return Err(PickError::MergeCommitUnsupported);
    }
    let staged = repo.diff_tree_to_index(Some(&head.tree()?), None, None)?;
    if staged.deltas().len() > 0 {
        return Err(PickError::DirtyIndex);
    }

    // libgit2's default checkout strategy here is SAFE: it errors rather than overwrite a
    // modified working-tree file the pick needs to change. libgit2 may already have written
    // CHERRY_PICK_HEAD/REVERT_HEAD + MERGE_MSG by then; state was Clean on entry (checked above),
    // so clearing those files restores exactly the prior state without touching user files.
    let applied = match kind {
        Kind::CherryPick => repo.cherrypick(&target, None),
        Kind::Revert => repo.revert(&target, None),
    };
    if let Err(error) = applied {
        let _ = repo.cleanup_state();
        return Err(error.into());
    }

    let mut index = repo.index()?;
    if index.has_conflicts() {
        let mut files: Vec<String> = index
            .conflicts()?
            .filter_map(|conflict| conflict.ok())
            .filter_map(|conflict| conflict.our.or(conflict.their).or(conflict.ancestor))
            .map(|entry| String::from_utf8_lossy(&entry.path).into_owned())
            .collect();
        files.sort();
        files.dedup();
        return Ok(PickOutcome::Conflicted { files });
    }

    let tree_id = index.write_tree()?;
    if tree_id == head.tree_id() {
        repo.cleanup_state()?;
        return Err(PickError::EmptyResult);
    }
    let tree = repo.find_tree(tree_id)?;
    let committer = repo.signature()?;
    let (author, message) = match kind {
        Kind::CherryPick => (target.author().to_owned(), target.message().unwrap_or("").to_string()),
        Kind::Revert => (committer.clone(), revert_message(&target)),
    };
    let oid = repo.commit(Some("HEAD"), &author, &committer, &message, &tree, &[&head])?;
    repo.cleanup_state()?;
    Ok(PickOutcome::Committed { commit_id: oid.to_string() })
}

fn revert_message(target: &Commit<'_>) -> String {
    format!(
        "Revert \"{}\"\n\nThis reverts commit {}.\n",
        target.summary().unwrap_or(""),
        target.id()
    )
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p git-core --test pick`
Expected: 9 passed. If `cherry_pick_never_overwrites_an_unstaged_edit_to_a_touched_file` fails (libgit2 returned `Ok` with a modified index, or modified the file), do **not** reset anything after the fact — that would touch user state. Instead add an up-front check before the `cherrypick`/`revert` call: collect the working-tree-modified paths (`repo.diff_index_to_workdir(None, None)` deltas' `new_file().path()`) and the target's changed paths (`repo.diff_tree_to_tree(target.parent(0).ok().map(|p| p.tree()).transpose()?.as_ref(), Some(&target.tree()?), None)` deltas), and if they intersect return `PickError::Git(git2::Error::from_str(&format!("Your local changes to {path} would be overwritten. Commit or stash them first.")))`. Re-run.

- [ ] **Step 5: Commit**

```bash
git add crates/git-core/src/lib.rs crates/git-core/src/pick.rs crates/git-core/tests/pick.rs
git commit -m "feat(git-core): cherry_pick and revert with merge-style conflict pause"
```

---

### Task 4: Reset and detached checkout

**Files:**
- Create: `crates/git-core/src/reset.rs`
- Modify: `crates/git-core/src/lib.rs` (add `pub mod reset;`), `crates/git-core/src/branch.rs`
- Test: `crates/git-core/tests/reset.rs`, `crates/git-core/tests/checkout_commit.rs` (create)

**Interfaces:**
- Produces:
  ```rust
  // reset.rs
  pub enum ResetMode { Soft, Mixed, Hard }   // Debug, Clone, Copy, PartialEq, Eq
  pub enum ResetError { Git(git2::Error), OperationInProgress }
  pub fn reset_to(repo: &git2::Repository, commit_id: &str, mode: ResetMode) -> Result<(), ResetError>
  // branch.rs
  pub fn checkout_commit(repo: &git2::Repository, commit_id: &str) -> Result<(), BranchError>
  ```

- [ ] **Step 1: Write the failing tests**

```rust
// crates/git-core/tests/reset.rs
mod common;

use git_core::reset::{reset_to, ResetError, ResetMode};

fn three_commits() -> (tempfile::TempDir, git2::Repository, git2::Oid) {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "1\n");
    common::commit_all(&repo, "one");
    let first = repo.head().unwrap().target().unwrap();
    common::write_file(dir.path(), "a.txt", "2\n");
    common::commit_all(&repo, "two");
    common::write_file(dir.path(), "a.txt", "3\n");
    common::commit_all(&repo, "three");
    (dir, repo, first)
}

fn staged_paths(repo: &git2::Repository) -> usize {
    let head_tree = repo.head().unwrap().peel_to_tree().unwrap();
    repo.diff_tree_to_index(Some(&head_tree), None, None).unwrap().deltas().len()
}

#[test]
fn soft_reset_moves_branch_and_keeps_index_and_workdir() {
    let (dir, repo, first) = three_commits();
    reset_to(&repo, &first.to_string(), ResetMode::Soft).unwrap();
    assert_eq!(repo.head().unwrap().target().unwrap(), first);
    assert!(repo.head().unwrap().is_branch());
    assert_eq!(staged_paths(&repo), 1);
    assert_eq!(std::fs::read_to_string(dir.path().join("a.txt")).unwrap(), "3\n");
}

#[test]
fn mixed_reset_unstages_but_keeps_workdir() {
    let (dir, repo, first) = three_commits();
    reset_to(&repo, &first.to_string(), ResetMode::Mixed).unwrap();
    assert_eq!(staged_paths(&repo), 0);
    assert_eq!(std::fs::read_to_string(dir.path().join("a.txt")).unwrap(), "3\n");
}

#[test]
fn hard_reset_discards_tracked_changes_but_keeps_untracked_files() {
    let (dir, repo, first) = three_commits();
    common::write_file(dir.path(), "untracked.txt", "keep me\n");
    reset_to(&repo, &first.to_string(), ResetMode::Hard).unwrap();
    assert_eq!(std::fs::read_to_string(dir.path().join("a.txt")).unwrap(), "1\n");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("untracked.txt")).unwrap(),
        "keep me\n"
    );
}

#[test]
fn reset_refuses_during_a_paused_operation() {
    let (_dir, repo, first) = three_commits();
    let head = repo.head().unwrap().target().unwrap();
    std::fs::write(repo.path().join("MERGE_HEAD"), format!("{head}\n")).unwrap();
    assert!(matches!(
        reset_to(&repo, &first.to_string(), ResetMode::Hard),
        Err(ResetError::OperationInProgress)
    ));
}
```

```rust
// crates/git-core/tests/checkout_commit.rs
mod common;

use git_core::branch::checkout_commit;

#[test]
fn checks_out_a_commit_as_detached_head() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "1\n");
    common::commit_all(&repo, "one");
    let first = repo.head().unwrap().target().unwrap();
    common::write_file(dir.path(), "a.txt", "2\n");
    common::commit_all(&repo, "two");

    checkout_commit(&repo, &first.to_string()).unwrap();

    assert!(repo.head_detached().unwrap());
    assert_eq!(repo.head().unwrap().target().unwrap(), first);
    assert_eq!(std::fs::read_to_string(dir.path().join("a.txt")).unwrap(), "1\n");
}

#[test]
fn refuses_to_overwrite_local_changes_and_leaves_head_attached() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "1\n");
    common::commit_all(&repo, "one");
    let first = repo.head().unwrap().target().unwrap();
    common::write_file(dir.path(), "a.txt", "2\n");
    common::commit_all(&repo, "two");
    let second = repo.head().unwrap().target().unwrap();
    common::write_file(dir.path(), "a.txt", "dirty\n");

    assert!(checkout_commit(&repo, &first.to_string()).is_err());

    assert!(!repo.head_detached().unwrap());
    assert_eq!(repo.head().unwrap().target().unwrap(), second);
    assert_eq!(std::fs::read_to_string(dir.path().join("a.txt")).unwrap(), "dirty\n");
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p git-core --test reset --test checkout_commit`
Expected: compile errors (missing module/function).

- [ ] **Step 3: Implement**

`crates/git-core/src/reset.rs`:

```rust
use git2::{Oid, Repository, RepositoryState, ResetType};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetMode {
    Soft,
    Mixed,
    Hard,
}

#[derive(Debug, Error)]
pub enum ResetError {
    #[error("{0}")]
    Git(#[from] git2::Error),
    #[error("Finish or abort the current merge/cherry-pick/revert/rebase first.")]
    OperationInProgress,
}

/// Moves the current branch (or detached HEAD) to `commit_id`. Soft keeps index + working tree,
/// Mixed resets the index only, Hard resets both — untracked files are never removed (same as
/// `git reset --hard`).
pub fn reset_to(repo: &Repository, commit_id: &str, mode: ResetMode) -> Result<(), ResetError> {
    if repo.state() != RepositoryState::Clean {
        return Err(ResetError::OperationInProgress);
    }
    let target = repo.find_commit(Oid::from_str(commit_id)?)?;
    let kind = match mode {
        ResetMode::Soft => ResetType::Soft,
        ResetMode::Mixed => ResetType::Mixed,
        ResetMode::Hard => ResetType::Hard,
    };
    repo.reset(target.as_object(), kind, None)?;
    Ok(())
}
```

Add `pub mod reset;` to `lib.rs` (after `repo`). In `branch.rs`, after `switch_branch`:

```rust
/// Checks out `commit_id` as a detached HEAD. Same refuse-before-move ordering as
/// `switch_branch`: the tree checkout (safe strategy) runs first, so a refused checkout leaves
/// HEAD and the working tree untouched.
pub fn checkout_commit(repo: &Repository, commit_id: &str) -> Result<(), BranchError> {
    let target = repo.find_commit(git2::Oid::from_str(commit_id)?)?;
    repo.checkout_tree(target.as_object(), None)?;
    repo.set_head_detached(target.id())?;
    Ok(())
}
```

(`BranchError` already has a `#[from] git2::Error` variant — verify at `branch.rs:5`; if its variant name differs, use it.)

- [ ] **Step 4: Run tests**

Run: `cargo test -p git-core --test reset --test checkout_commit --test branch`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/git-core/src/lib.rs crates/git-core/src/reset.rs crates/git-core/src/branch.rs crates/git-core/tests/reset.rs crates/git-core/tests/checkout_commit.rs
git commit -m "feat(git-core): reset_to (soft/mixed/hard) and checkout_commit (detached HEAD)"
```

---

### Task 5: Force push with lease

**Files:**
- Modify: `crates/git-core/src/remote.rs`
- Test: `crates/git-core/tests/remote.rs`

**Interfaces:**
- Produces:
  ```rust
  TransferOperation::ForcePushBranch          // new variant
  TransferErrorKind::StaleLease               // new variant
  RemoteError::StaleLease                     // new variant, #[error("the remote branch changed since the last fetch")]
  pub fn force_push_current_branch(
      repo: &Repository, remote_name: &str,
      credentials: &mut dyn CredentialProvider, reporter: &mut dyn TransferReporter,
      cancel: &dyn Fn() -> bool,
  ) -> Result<(), RemoteError>
  ```
- `push_refs` gains a trailing `lease: Option<(&str /* dst refname */, git2::Oid /* expected, zero = must not exist */)>` parameter; existing callers pass `None`.

- [ ] **Step 1: Write the failing tests** (append to `crates/git-core/tests/remote.rs`; add `force_push_current_branch` to the `use git_core::remote::{…}` list)

```rust
impl RemoteFixture {
    /// Like `remote_commit`, but does NOT fetch into `local` — the local tracking ref goes stale.
    fn remote_commit_without_fetch(&self, message: &str) {
        common::write_file(self.source_dir.path(), "remote.txt", message);
        common::commit_all(&self.source, message);
        let branch = self.source.head().unwrap().shorthand().unwrap().to_string();
        let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
        self.source.find_remote("origin").unwrap().push(&[refspec], None).unwrap();
    }

    fn bare_tip(&self, branch: &str) -> Option<git2::Oid> {
        let bare = git2::Repository::open_bare(self.remote_dir.path()).unwrap();
        bare.find_reference(&format!("refs/heads/{branch}")).ok().and_then(|r| r.target())
    }
}

#[test]
fn force_push_overwrites_the_remote_when_the_lease_matches() {
    let fixture = local_and_bare_remote();
    fixture.remote_commit("remote change"); // fetched: tracking ref up to date
    fixture.local_commit("local change");   // diverged
    let mut reporter = VecReporter::default();

    force_push_current_branch(&fixture.local, "origin", &mut NoCredentials, &mut reporter, &|| false)
        .unwrap();

    let local_tip = fixture.local.head().unwrap().target().unwrap();
    assert_eq!(fixture.bare_tip("main"), Some(local_tip));
    assert!(reporter.events.iter().all(|e| e.operation == TransferOperation::ForcePushBranch));
}

#[test]
fn force_push_refuses_a_stale_lease_and_leaves_the_remote_untouched() {
    let fixture = local_and_bare_remote();
    fixture.remote_commit_without_fetch("someone else's work");
    fixture.local_commit("local change");
    let remote_before = fixture.bare_tip("main");
    let mut reporter = VecReporter::default();

    let result = force_push_current_branch(
        &fixture.local, "origin", &mut NoCredentials, &mut reporter, &|| false,
    );

    assert!(matches!(result, Err(RemoteError::StaleLease)));
    assert_eq!(
        result.unwrap_err().transfer_error_kind(),
        TransferErrorKind::StaleLease
    );
    assert_eq!(fixture.bare_tip("main"), remote_before);
}

#[test]
fn force_push_of_a_branch_new_to_the_remote_succeeds() {
    let fixture = local_and_bare_remote();
    let head = fixture.local.head().unwrap().peel_to_commit().unwrap();
    fixture.local.branch("brand-new", &head, false).unwrap();
    fixture.local.set_head("refs/heads/brand-new").unwrap();
    fixture.local_commit("on new branch");
    let mut reporter = VecReporter::default();

    force_push_current_branch(&fixture.local, "origin", &mut NoCredentials, &mut reporter, &|| false)
        .unwrap();

    assert_eq!(
        fixture.bare_tip("brand-new"),
        Some(fixture.local.head().unwrap().target().unwrap())
    );
}

#[test]
fn force_push_refuses_when_remote_has_the_branch_but_it_was_never_fetched() {
    let fixture = local_and_bare_remote();
    // Remote gains `other`; local never fetches it, then creates its own `other`.
    let src_head = fixture.source.head().unwrap().peel_to_commit().unwrap();
    fixture.source.branch("other", &src_head, false).unwrap();
    fixture.source.find_remote("origin").unwrap()
        .push(&["refs/heads/other:refs/heads/other"], None).unwrap();
    let head = fixture.local.head().unwrap().peel_to_commit().unwrap();
    fixture.local.branch("other", &head, false).unwrap();
    fixture.local.set_head("refs/heads/other").unwrap();
    fixture.local_commit("local other");
    let mut reporter = VecReporter::default();

    assert!(matches!(
        force_push_current_branch(&fixture.local, "origin", &mut NoCredentials, &mut reporter, &|| false),
        Err(RemoteError::StaleLease)
    ));
}

#[test]
fn force_push_refuses_a_detached_head() {
    let fixture = local_and_bare_remote();
    let oid = fixture.local.head().unwrap().target().unwrap();
    fixture.local.set_head_detached(oid).unwrap();
    let mut reporter = VecReporter::default();
    assert!(matches!(
        force_push_current_branch(&fixture.local, "origin", &mut NoCredentials, &mut reporter, &|| false),
        Err(RemoteError::DetachedHead)
    ));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p git-core --test remote force_push`
Expected: compile error (`force_push_current_branch`, `StaleLease`, `ForcePushBranch` missing).

- [ ] **Step 3: Implement**

In `remote.rs`:
1. Add `ForcePushBranch` to `TransferOperation` (after `PushBranch`), `StaleLease` to `TransferErrorKind` (after `RejectedRemoteRef`).
2. Add to `RemoteError` (after `RejectedRemoteRef`):
   ```rust
   #[error("the remote branch changed since the last fetch")]
   StaleLease,
   ```
   and to `transfer_error_kind`: `Self::StaleLease => TransferErrorKind::StaleLease,`.
3. In `push_refs`' final `match rejection.get()`, add `Some(TransferErrorKind::StaleLease) => unreachable!(),` (a lease failure aborts at negotiation, never as a per-ref rejection).
4. Add the `lease` parameter to `push_refs` and update `push_current_branch` / `push_tags` to pass `None`. Inside `push_refs`, replace the `push_negotiation` callback with:

```rust
    let cancelled_at_negotiation = Cell::new(false);
    let stale_lease = Cell::new(false);
    callbacks.push_negotiation(|updates| {
        if cancel() {
            cancelled_at_negotiation.set(true);
            return Err(git2::Error::from_str("transfer cancelled"));
        }
        // Force-with-lease: `src()` is what the remote advertised for the destination ref
        // (zero when absent). This runs after advertisement and before any pack data is sent,
        // so refusing here leaves the remote untouched — the same window
        // `git push --force-with-lease` has.
        if let Some((dst, expected)) = lease {
            let current = updates
                .iter()
                .find(|update| update.dst_refname() == Some(dst))
                .map(|update| update.src())
                .unwrap_or_else(git2::Oid::zero);
            if current != expected {
                stale_lease.set(true);
                return Err(git2::Error::from_str("stale lease"));
            }
        }
        Ok(())
    });
```

   and in the `if let Err(error) = pushed` branch, check `stale_lease` first:

```rust
    if let Err(error) = pushed {
        return if stale_lease.get() {
            Err(RemoteError::StaleLease)
        } else if cancelled_at_negotiation.get() {
            Err(RemoteError::Cancelled)
        } else if error.code() == ErrorCode::NotFastForward {
            Err(RemoteError::NonFastForward)
        } else {
            Err(error.into())
        };
    }
```

5. Add after `push_current_branch`:

```rust
/// Force-pushes the current branch, but only if the remote's branch still points where our
/// `refs/remotes/<remote>/<branch>` says it did at the last fetch (a missing tracking ref means
/// "must not exist on the remote"). A mismatch is `RemoteError::StaleLease`, remote untouched.
pub fn force_push_current_branch(
    repo: &Repository,
    remote_name: &str,
    credentials: &mut dyn CredentialProvider,
    reporter: &mut dyn TransferReporter,
    cancel: &dyn Fn() -> bool,
) -> Result<(), RemoteError> {
    let branch = current_local_branch_name(repo)?;
    let expected = repo
        .find_reference(&format!("refs/remotes/{remote_name}/{branch}"))
        .ok()
        .and_then(|reference| reference.target())
        .unwrap_or_else(git2::Oid::zero);
    let dst = format!("refs/heads/{branch}");
    let refspec = format!("+refs/heads/{branch}:{dst}");
    push_refs(
        repo,
        remote_name,
        &[refspec],
        TransferOperation::ForcePushBranch,
        credentials,
        reporter,
        cancel,
        Some((dst.as_str(), expected)),
    )
}
```

   If `push_refs` updates the tracking ref only for non-forced pushes, verify after a successful force push that `refs/remotes/<remote>/<branch>` equals the new local tip (libgit2 updates it via `push_update_reference` for any accepted update); add an assertion to `force_push_overwrites_the_remote_when_the_lease_matches`:
   ```rust
   assert_eq!(fixture.local.find_reference("refs/remotes/origin/main").unwrap().target(), Some(local_tip));
   ```
   If it fails, update the tracking ref explicitly after a successful push in `force_push_current_branch`.

   Clippy may flag `push_refs` for too many arguments — add `#[allow(clippy::too_many_arguments)]` on `push_refs`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p git-core --test remote`
Expected: all pass (existing push/cancel tests unaffected).

- [ ] **Step 5: Commit**

```bash
git add crates/git-core/src/remote.rs crates/git-core/tests/remote.rs
git commit -m "feat(git-core): force_push_current_branch with lease checked at push negotiation"
```

---

### Task 6: repo-service worker commands

**Files:**
- Create: `crates/repo-service/src/worker/history.rs`
- Modify: `crates/repo-service/src/worker/mod.rs` (`mod history;`, `Command` variants, dispatch arms, round-trip tests), `crates/repo-service/src/worker/remote.rs` (`force_push_branch` + `WorkerHandle::force_push_current_branch`)

**Interfaces:**
- Consumes: Tasks 1–5 functions.
- Produces (`WorkerHandle` methods):
  ```rust
  pub fn get_head_info(&self) -> Result<git_core::repo::HeadInfo, String>
  pub fn amend_commit(&self, message: String) -> Result<(), String>
  pub fn cherry_pick(&self, commit_id: String) -> Result<git_core::pick::PickOutcome, String>
  pub fn revert_commit(&self, commit_id: String) -> Result<git_core::pick::PickOutcome, String>
  pub fn reset_to_commit(&self, commit_id: String, mode: git_core::reset::ResetMode) -> Result<(), String>
  pub fn checkout_commit(&self, commit_id: String) -> Result<(), String>
  pub fn force_push_current_branch(&self, remote_name: String, events: Sender<TransferEvent>) -> Result<String, String> // operation id "force-push-N"
  ```

- [ ] **Step 1: Write the failing tests** — append to the `#[cfg(test)]` module of `worker/mod.rs` (helpers `init_repo`, `write_file`, `commit_all`, `local_and_bare_remote` are already in scope there; see `abort_merge_round_trips_through_the_worker` and `non_fast_forward_branch_push_streams_a_safe_terminal_error_kind`).

```rust
    #[test]
    fn history_commands_round_trip_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "base.txt", "base\n");
        commit_all(&repo, "base commit");
        let main_branch = git_core::branch::list_branches(&repo).unwrap()[0].name.clone();
        let base = repo.head().unwrap().target().unwrap().to_string();
        git_core::branch::create_branch(&repo, "side", "HEAD").unwrap();
        git_core::branch::switch_branch(&repo, "side").unwrap();
        write_file(dir.path(), "side.txt", "side\n");
        commit_all(&repo, "side commit");
        let side = repo.head().unwrap().target().unwrap().to_string();
        git_core::branch::switch_branch(&repo, &main_branch).unwrap();

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();

        assert_eq!(handle.get_head_info().unwrap().head_id, Some(base.clone()));

        let outcome = handle.cherry_pick(side.clone()).unwrap();
        assert!(matches!(outcome, git_core::pick::PickOutcome::Committed { .. }));

        handle.amend_commit("side, amended".into()).unwrap();
        let picked = handle.get_head_info().unwrap().head_id.unwrap();

        let reverted = handle.revert_commit(picked).unwrap();
        assert!(matches!(reverted, git_core::pick::PickOutcome::Committed { .. }));

        handle
            .reset_to_commit(base.clone(), git_core::reset::ResetMode::Hard)
            .unwrap();
        assert_eq!(handle.get_head_info().unwrap().head_id, Some(base.clone()));

        handle.checkout_commit(side).unwrap();
        assert!(handle.get_head_info().unwrap().detached_at.is_some());

        assert!(handle.cherry_pick("not-a-sha".into()).is_err());
        assert!(handle.get_head_info().is_ok(), "worker survives a failed command");
    }

    #[test]
    fn force_push_streams_a_completed_force_push_event() {
        let (_source_dir, _remote_dir, local_dir) = local_and_bare_remote();
        let local = Repository::open(local_dir.path()).expect("open local repo");
        write_file(local_dir.path(), "README.md", "rewritten local history\n");
        commit_all(&local, "local commit");
        drop(local);
        let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
        let (event_tx, event_rx) = mpsc::channel();

        let operation_id = worker
            .handle()
            .force_push_current_branch("origin".into(), event_tx)
            .expect("start force push");
        let events: Vec<_> = event_rx.iter().collect();

        assert!(operation_id.starts_with("force-push-"));
        assert!(matches!(
            events.last(),
            Some(TransferEvent::Completed {
                operation_id: id,
                operation: TransferOperation::ForcePushBranch,
                error: None,
            }) if id == &operation_id
        ));
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p repo-service history_commands_round_trip`
Expected: compile error (methods missing).

- [ ] **Step 3: Implement**

`crates/repo-service/src/worker/history.rs` (handlers + handle methods, same shape as `worker/merge.rs`):

```rust
use super::{Command, WorkerHandle};
use git_core::pick::PickOutcome;
use git_core::repo::HeadInfo;
use git_core::reset::ResetMode;
use std::sync::mpsc::Sender;

pub(super) fn head_info(repo: &git2::Repository, reply: Sender<Result<HeadInfo, String>>) {
    let _ = reply.send(git_core::repo::head_info(repo).map_err(|error| error.to_string()));
}
pub(super) fn amend(repo: &git2::Repository, message: String, reply: Sender<Result<(), String>>) {
    let _ = reply.send(
        git_core::commit::amend_head(repo, &message).map(|_| ()).map_err(|error| error.to_string()),
    );
}
pub(super) fn cherry_pick(
    repo: &git2::Repository,
    commit_id: String,
    reply: Sender<Result<PickOutcome, String>>,
) {
    let _ = reply.send(git_core::pick::cherry_pick(repo, &commit_id).map_err(|error| error.to_string()));
}
pub(super) fn revert(
    repo: &git2::Repository,
    commit_id: String,
    reply: Sender<Result<PickOutcome, String>>,
) {
    let _ = reply.send(git_core::pick::revert(repo, &commit_id).map_err(|error| error.to_string()));
}
pub(super) fn reset(
    repo: &git2::Repository,
    commit_id: String,
    mode: ResetMode,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        git_core::reset::reset_to(repo, &commit_id, mode).map_err(|error| error.to_string()),
    );
}
pub(super) fn checkout(repo: &git2::Repository, commit_id: String, reply: Sender<Result<(), String>>) {
    let _ = reply.send(
        git_core::branch::checkout_commit(repo, &commit_id).map_err(|error| error.to_string()),
    );
}

fn round_trip<T>(handle: &WorkerHandle, build: impl FnOnce(Sender<Result<T, String>>) -> Command) -> Result<T, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    handle.tx.send(build(tx)).map_err(|_| "worker thread stopped".to_string())?;
    rx.recv().map_err(|_| "worker thread stopped before replying".to_string())?
}

impl WorkerHandle {
    pub fn get_head_info(&self) -> Result<HeadInfo, String> {
        round_trip(self, |reply| Command::GetHeadInfo { reply })
    }
    pub fn amend_commit(&self, message: String) -> Result<(), String> {
        round_trip(self, |reply| Command::AmendCommit { message, reply })
    }
    pub fn cherry_pick(&self, commit_id: String) -> Result<PickOutcome, String> {
        round_trip(self, |reply| Command::CherryPick { commit_id, reply })
    }
    pub fn revert_commit(&self, commit_id: String) -> Result<PickOutcome, String> {
        round_trip(self, |reply| Command::RevertCommit { commit_id, reply })
    }
    pub fn reset_to_commit(&self, commit_id: String, mode: ResetMode) -> Result<(), String> {
        round_trip(self, |reply| Command::ResetToCommit { commit_id, mode, reply })
    }
    pub fn checkout_commit(&self, commit_id: String) -> Result<(), String> {
        round_trip(self, |reply| Command::CheckoutCommit { commit_id, reply })
    }
}
```

In `worker/mod.rs`: add `mod history;`; add `Command` variants

```rust
    GetHeadInfo { reply: Sender<Result<git_core::repo::HeadInfo, String>> },
    AmendCommit { message: String, reply: Sender<Result<(), String>> },
    CherryPick { commit_id: String, reply: Sender<Result<git_core::pick::PickOutcome, String>> },
    RevertCommit { commit_id: String, reply: Sender<Result<git_core::pick::PickOutcome, String>> },
    ResetToCommit { commit_id: String, mode: git_core::reset::ResetMode, reply: Sender<Result<(), String>> },
    CheckoutCommit { commit_id: String, reply: Sender<Result<(), String>> },
    ForcePushCurrentBranch {
        remote_name: String,
        operation_id: String,
        events: Sender<TransferEvent>,
        reply: Sender<Result<String, String>>,
    },
```

and dispatch arms next to the merge arms:

```rust
                    Command::GetHeadInfo { reply } => history::head_info(&repo, reply),
                    Command::AmendCommit { message, reply } => history::amend(&repo, message, reply),
                    Command::CherryPick { commit_id, reply } => history::cherry_pick(&repo, commit_id, reply),
                    Command::RevertCommit { commit_id, reply } => history::revert(&repo, commit_id, reply),
                    Command::ResetToCommit { commit_id, mode, reply } => {
                        history::reset(&repo, commit_id, mode, reply)
                    }
                    Command::CheckoutCommit { commit_id, reply } => history::checkout(&repo, commit_id, reply),
```

plus a `Command::ForcePushCurrentBranch { .. }` arm that copies the `Command::PushCurrentBranch` arm (line ~701) verbatim but calls `remote::force_push_branch`.

In `worker/remote.rs`: copy `push_branch` to `force_push_branch`, replacing both `TransferOperation::PushBranch` with `TransferOperation::ForcePushBranch` and `git_core::remote::push_current_branch` with `git_core::remote::force_push_current_branch`. Copy `WorkerHandle::push_current_branch` to `force_push_current_branch`, with id `format!("force-push-{}", NEXT_TRANSFER_ID.fetch_add(1, Ordering::Relaxed))` and `Command::ForcePushCurrentBranch`.

If `round_trip` can't see `handle.tx` (private field in another module), make the field `pub(super)` or place `round_trip` in `mod.rs` — match whichever visibility `worker/merge.rs` already relies on (it uses `self.tx` inside `impl WorkerHandle`, so it is visible from sibling modules).

- [ ] **Step 4: Run tests**

Run: `cargo test -p repo-service`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/repo-service/src/worker/
git commit -m "feat(repo-service): worker commands for head info, amend, cherry-pick, revert, reset, detached checkout, force push"
```

---

### Task 7: Tauri commands and sidecar methods

**Files:**
- Create: `crates/tauri-app/src/commands/history.rs`
- Modify: `crates/tauri-app/src/commands/mod.rs` (DTOs + `pub mod history;` + re-exports + serialization tests), `crates/tauri-app/src/commands/remote.rs` (`force_push_current_branch`), `crates/tauri-app/src/main.rs` (register 7 commands), `crates/vscode-sidecar/src/dispatch.rs` (7 methods + DTOs), `crates/vscode-sidecar/tests/protocol_roundtrip.rs`

**Interfaces:**
- Wire names (snake_case commands; camelCase params; `repoPath` first):
  `get_head_info {repoPath}`, `amend_commit {repoPath, message}`, `cherry_pick {repoPath, commitId}`, `revert_commit {repoPath, commitId}`, `reset_to_commit {repoPath, commitId, mode}`, `checkout_commit {repoPath, commitId}`, `force_push_current_branch {repoPath, remoteName}`.
- DTOs (identical in `tauri-app` and `vscode-sidecar`):
  ```rust
  #[derive(Serialize)] #[serde(rename_all = "camelCase")]
  pub struct HeadInfoDto { head_id: Option<String>, detached_at: Option<String>, head_pushed: bool, operation: Option<&'static str> } // "Merge" | "CherryPick" | "Revert"
  #[derive(Serialize)] #[serde(tag = "kind")]
  pub enum PickOutcomeDto { Committed { #[serde(rename = "commitId")] commit_id: String }, Conflicted { files: Vec<String> } }
  #[derive(Deserialize)] #[serde(rename_all = "lowercase")]
  pub enum ResetModeDto { Soft, Mixed, Hard }
  ```
  JSON: `{"headId":…, "detachedAt":…, "headPushed":…, "operation":…}`, `{"kind":"Committed","commitId":"…"}`, `{"kind":"Conflicted","files":[…]}`, mode `"soft" | "mixed" | "hard"`.

- [ ] **Step 1: Write the failing tests**

In `tauri-app/src/commands/mod.rs`'s test module:

```rust
    #[test]
    fn head_info_dto_serializes_camel_case_with_operation_name() {
        let dto = HeadInfoDto::from(git_core::repo::HeadInfo {
            head_id: Some("abc".into()),
            detached_at: None,
            head_pushed: true,
            operation: Some(git_core::repo::OperationKind::CherryPick),
        });
        assert_eq!(
            serde_json::to_value(dto).unwrap(),
            serde_json::json!({"headId": "abc", "detachedAt": null, "headPushed": true, "operation": "CherryPick"})
        );
    }

    #[test]
    fn pick_outcome_dto_is_kind_tagged() {
        assert_eq!(
            serde_json::to_value(PickOutcomeDto::from(git_core::pick::PickOutcome::Committed { commit_id: "c1".into() })).unwrap(),
            serde_json::json!({"kind": "Committed", "commitId": "c1"})
        );
        assert_eq!(
            serde_json::to_value(PickOutcomeDto::from(git_core::pick::PickOutcome::Conflicted { files: vec!["a".into()] })).unwrap(),
            serde_json::json!({"kind": "Conflicted", "files": ["a"]})
        );
    }

    #[test]
    fn reset_mode_dto_parses_lowercase() {
        let mode: ResetModeDto = serde_json::from_value(serde_json::json!("hard")).unwrap();
        assert_eq!(git_core::reset::ResetMode::from(mode), git_core::reset::ResetMode::Hard);
    }

    #[test]
    fn force_push_and_stale_lease_names_reach_the_wire() {
        // Same shape as the existing PushBranch failed-event test near line 1282.
        let failed = TransferProgressDto::from(/* Completed event, operation ForcePushBranch, error Some(StaleLease) — build exactly like the existing test */);
        assert_eq!(failed.operation, "ForcePushBranch");
        assert_eq!(failed.error_kind.as_deref(), Some("StaleLease"));
    }
```

(For the last test, open the existing test around `mod.rs:1282` and copy its event construction, changing only the operation and error kind.)

In `vscode-sidecar/tests/protocol_roundtrip.rs`, add a test following the existing `start_merge`/`abort_merge` round-trip tests: open a temp repo with two commits through the sidecar, call `get_head_info` (assert `headId` is a 40-char string, `operation` null), `checkout_commit` the first commit (then `get_head_info` has non-null `detachedAt`), `reset_to_commit` with `"mode":"hard"` → `null` result. And one `cherry_pick` call returning `{"kind":"Committed",…}` from a side branch.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p tauri-app && cargo test -p vscode-sidecar`
Expected: compile errors (DTOs/commands missing).

- [ ] **Step 3: Implement**

`crates/tauri-app/src/commands/mod.rs` — add DTOs next to `MergeOutcomeDto`:

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadInfoDto {
    pub head_id: Option<String>,
    pub detached_at: Option<String>,
    pub head_pushed: bool,
    pub operation: Option<&'static str>,
}

impl From<git_core::repo::HeadInfo> for HeadInfoDto {
    fn from(info: git_core::repo::HeadInfo) -> Self {
        use git_core::repo::OperationKind;
        Self {
            head_id: info.head_id,
            detached_at: info.detached_at,
            head_pushed: info.head_pushed,
            operation: info.operation.map(|kind| match kind {
                OperationKind::Merge => "Merge",
                OperationKind::CherryPick => "CherryPick",
                OperationKind::Revert => "Revert",
            }),
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind")]
pub enum PickOutcomeDto {
    Committed {
        #[serde(rename = "commitId")]
        commit_id: String,
    },
    Conflicted { files: Vec<String> },
}

impl From<git_core::pick::PickOutcome> for PickOutcomeDto {
    fn from(outcome: git_core::pick::PickOutcome) -> Self {
        match outcome {
            git_core::pick::PickOutcome::Committed { commit_id } => Self::Committed { commit_id },
            git_core::pick::PickOutcome::Conflicted { files } => Self::Conflicted { files },
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResetModeDto {
    Soft,
    Mixed,
    Hard,
}

impl From<ResetModeDto> for git_core::reset::ResetMode {
    fn from(dto: ResetModeDto) -> Self {
        match dto {
            ResetModeDto::Soft => Self::Soft,
            ResetModeDto::Mixed => Self::Mixed,
            ResetModeDto::Hard => Self::Hard,
        }
    }
}
```

`crates/tauri-app/src/commands/history.rs`:

```rust
use super::{worker_handle, AppState, HeadInfoDto, PickOutcomeDto, ResetModeDto};
use tauri::State;

#[tauri::command]
pub async fn get_head_info(repo_path: String, state: State<'_, AppState>) -> Result<HeadInfoDto, String> {
    Ok(HeadInfoDto::from(worker_handle(&state, &repo_path)?.get_head_info()?))
}
#[tauri::command]
pub async fn amend_commit(repo_path: String, message: String, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.amend_commit(message)
}
#[tauri::command]
pub async fn cherry_pick(repo_path: String, commit_id: String, state: State<'_, AppState>) -> Result<PickOutcomeDto, String> {
    Ok(PickOutcomeDto::from(worker_handle(&state, &repo_path)?.cherry_pick(commit_id)?))
}
#[tauri::command]
pub async fn revert_commit(repo_path: String, commit_id: String, state: State<'_, AppState>) -> Result<PickOutcomeDto, String> {
    Ok(PickOutcomeDto::from(worker_handle(&state, &repo_path)?.revert_commit(commit_id)?))
}
#[tauri::command]
pub async fn reset_to_commit(
    repo_path: String,
    commit_id: String,
    mode: ResetModeDto,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.reset_to_commit(commit_id, mode.into())
}
#[tauri::command]
pub async fn checkout_commit(repo_path: String, commit_id: String, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.checkout_commit(commit_id)
}
```

Add `pub mod history;` and `pub use history::{amend_commit, checkout_commit, cherry_pick, get_head_info, reset_to_commit, revert_commit};` to `commands/mod.rs` (follow how `merge` is declared/re-exported at line ~31). In `commands/remote.rs`, copy `push_current_branch` to `force_push_current_branch` calling `.force_push_current_branch(remote_name, event_tx)`. Register all 7 in `main.rs`'s `use` list and `generate_handler![…]`.

`crates/vscode-sidecar/src/dispatch.rs`: add identical `HeadInfoDto`/`PickOutcomeDto`/`ResetModeDto` (sidecar keeps its own DTO copies — see `ConflictSegmentDto` there), param structs

```rust
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommitIdParams { repo_path: String, commit_id: String }
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MessageParams { repo_path: String, message: String }
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResetParams { repo_path: String, commit_id: String, mode: ResetModeDto }
```

(reuse an existing struct if one with identical fields already exists — grep `struct .*Params` first), dispatch entries

```rust
        "get_head_info" => get_head_info(params, repos),
        "amend_commit" => amend_commit(params, repos),
        "cherry_pick" => cherry_pick(params, repos),
        "revert_commit" => revert_commit(params, repos),
        "reset_to_commit" => reset_to_commit(params, repos),
        "checkout_commit" => checkout_commit(params, repos),
        "force_push_current_branch" => force_push_current_branch(params, repos, stdout),
```

and handlers in the style of `start_merge` / `abort_merge` / `push_current_branch` (lines 1407–1665), e.g.:

```rust
fn cherry_pick(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: CommitIdParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
    let outcome = worker_handle(repos, &params.repo_path)?.cherry_pick(params.commit_id)?;
    serde_json::to_value(PickOutcomeDto::from(outcome)).map_err(|error| error.to_string())
}
```

Write all seven handlers out in full (no shared macro) to match the file's existing style.

- [ ] **Step 4: Run tests + lint**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Expected: all green.

- [ ] **Step 5: Commit**

```bash
git add crates/tauri-app crates/vscode-sidecar
git commit -m "feat(tauri-app, vscode-sidecar): expose history-rewrite commands and force push over IPC"
```

---

### Task 8: `RepoClient` surface

**Files:**
- Modify: `frontend/src/ipc/RepoClient.ts`, `frontend/src/ipc/tauriRepoClient.ts`, `frontend/src/ipc/vscodeRepoClient.ts`, `frontend/src/ipc/tauriRepoClient.test.ts`, `frontend/src/ipc/vscodeRepoClient.test.ts`, and every test file with a full `RepoClient` object literal (`App.test.tsx`, `state/useAppState.test.ts`, `components/RepoPicker.test.tsx`, `ConflictResolutionPane.test.tsx`, `DiffPane.test.tsx`, `RebasePlanner.test.tsx`, `CommitHeader.test.tsx`, `WorkspaceEditor.test.tsx`, `state/useOpenRepos.test.ts`, `state/useWorkspaces.test.ts` — run `pnpm exec tsc --noEmit` to find any others).

**Interfaces:**
- Produces (in `RepoClient.ts`):
  ```ts
  export type OperationKind = "Merge" | "CherryPick" | "Revert";
  export interface HeadInfo { headId: string | null; detachedAt: string | null; headPushed: boolean; operation: OperationKind | null; }
  export type PickOutcome = { kind: "Committed"; commitId: string } | { kind: "Conflicted"; files: string[] };
  export type ResetMode = "soft" | "mixed" | "hard";
  // RepoClient additions:
  getHeadInfo(repoPath: string): Promise<HeadInfo>;
  amendCommit(repoPath: string, message: string): Promise<void>;
  cherryPick(repoPath: string, commitId: string): Promise<PickOutcome>;
  revertCommit(repoPath: string, commitId: string): Promise<PickOutcome>;
  resetToCommit(repoPath: string, commitId: string, mode: ResetMode): Promise<void>;
  checkoutCommit(repoPath: string, commitId: string): Promise<void>;
  forcePushCurrentBranch(repoPath: string, remoteName: string): Promise<string>;
  ```
  `TransferErrorKind` gains `"StaleLease"`; `TransferProgress["operation"]` gains `"ForcePushBranch"`.

- [ ] **Step 1: Write the failing tests**

In `tauriRepoClient.test.ts`, following the existing per-method `invoke` assertions:

```ts
  it("invokes the history-rewrite commands with camelCase args", async () => {
    await client.cherryPick("/repo", "abc");
    expect(invoke).toHaveBeenLastCalledWith("cherry_pick", { repoPath: "/repo", commitId: "abc" });
    await client.revertCommit("/repo", "abc");
    expect(invoke).toHaveBeenLastCalledWith("revert_commit", { repoPath: "/repo", commitId: "abc" });
    await client.resetToCommit("/repo", "abc", "hard");
    expect(invoke).toHaveBeenLastCalledWith("reset_to_commit", { repoPath: "/repo", commitId: "abc", mode: "hard" });
    await client.checkoutCommit("/repo", "abc");
    expect(invoke).toHaveBeenLastCalledWith("checkout_commit", { repoPath: "/repo", commitId: "abc" });
    await client.amendCommit("/repo", "msg");
    expect(invoke).toHaveBeenLastCalledWith("amend_commit", { repoPath: "/repo", message: "msg" });
    await client.getHeadInfo("/repo");
    expect(invoke).toHaveBeenLastCalledWith("get_head_info", { repoPath: "/repo" });
    await client.forcePushCurrentBranch("/repo", "origin");
    expect(invoke).toHaveBeenLastCalledWith("force_push_current_branch", { repoPath: "/repo", remoteName: "origin" });
  });
```

(Match the existing test file's mock variable names — read its top first; `invoke` may be named `mockInvoke`.) Add the equivalent test to `vscodeRepoClient.test.ts` asserting the JSON-RPC `method`/`params` posted, following its existing pattern.

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/ipc`
Expected: FAIL (methods undefined).

- [ ] **Step 3: Implement**

Add types + interface methods to `RepoClient.ts` (methods after `getMergeMessage`). `tauriRepoClient.ts`, after `getMergeMessage`:

```ts
  getHeadInfo: (repoPath: string) => loggedInvoke<HeadInfo>("get_head_info", { repoPath }),
  amendCommit: (repoPath: string, message: string) => loggedInvoke("amend_commit", { repoPath, message }),
  cherryPick: (repoPath: string, commitId: string) => loggedInvoke<PickOutcome>("cherry_pick", { repoPath, commitId }),
  revertCommit: (repoPath: string, commitId: string) => loggedInvoke<PickOutcome>("revert_commit", { repoPath, commitId }),
  resetToCommit: (repoPath: string, commitId: string, mode: ResetMode) =>
    loggedInvoke("reset_to_commit", { repoPath, commitId, mode }),
  checkoutCommit: (repoPath: string, commitId: string) => loggedInvoke("checkout_commit", { repoPath, commitId }),
  forcePushCurrentBranch: (repoPath: string, remoteName: string) =>
    loggedInvoke<string>("force_push_current_branch", { repoPath, remoteName }),
```

`vscodeRepoClient.ts`: same seven with `call<…>(…)` in place of `loggedInvoke`. Add the new type imports to both.

For every full `RepoClient` literal in tests, add stubs matching that file's convention (`unused`, `unimplemented()`, or `async () => {}`); `getHeadInfo` returns a neutral value where `refresh` will call it (Task 9 makes `refresh` call it):

```ts
    getHeadInfo: async () => ({ headId: null, detachedAt: null, headPushed: false, operation: null }),
    amendCommit: async () => unimplemented(),
    cherryPick: async () => unimplemented(),
    revertCommit: async () => unimplemented(),
    resetToCommit: async () => unimplemented(),
    checkoutCommit: async () => unimplemented(),
    forcePushCurrentBranch: async () => unimplemented(),
```

- [ ] **Step 4: Run tests + typecheck + lint**

Run: `cd frontend && pnpm exec tsc --noEmit && pnpm lint && pnpm test -- --run`
Expected: all green.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): RepoClient methods for history rewrite, head info and force push"
```

---

### Task 9: State: `headInfo` + `useHistoryActions`

**Files:**
- Create: `frontend/src/state/useHistoryActions.ts`
- Modify: `frontend/src/state/useAppState.ts`
- Test: `frontend/src/state/useAppState.test.ts`

**Interfaces:**
- Consumes: Task 8 client methods; `RunMutation` from `useMutationRunner`.
- Produces:
  ```ts
  // AppState gains:
  headInfo: HeadInfo;                    // initial { headId: null, detachedAt: null, headPushed: false, operation: null }
  amendMode: boolean;                    // CommitBox amend toggle, initial false
  // UseAppStateResult gains (from useHistoryActions):
  cherryPick(commitId: string): Promise<void>;
  revertCommit(commitId: string): Promise<void>;
  resetToCommit(commitId: string, mode: ResetMode): Promise<void>;
  checkoutCommit(commitId: string): Promise<void>;
  amendCommit(message: string): Promise<void>;     // also turns amendMode off on success
  setAmendMode(on: boolean): void;
  ```
- `refresh()` fetches `client.getHeadInfo(repoPath)` in its existing `Promise.all` and stores it as `headInfo`.

- [ ] **Step 1: Write the failing tests** (append to `useAppState.test.ts`; reuse the file's `transferClient(overrides)` factory and its `renderHook` pattern — read an existing `mergeBranch` test first and mirror it)

```ts
describe("history actions", () => {
  it("loads headInfo on refresh", async () => {
    const info = { headId: "h1", detachedAt: null, headPushed: true, operation: null };
    const { result } = renderHook(() => useAppState(transferClient({ getHeadInfo: async () => info }), TEST_REPO_PATH));
    await act(() => result.current.refresh());
    expect(result.current.state.headInfo).toEqual(info);
  });

  it("cherryPick calls the client then refreshes", async () => {
    const cherryPick = vi.fn(async () => ({ kind: "Committed", commitId: "n1" }) as const);
    const getStatus = vi.fn(async () => []);
    const { result } = renderHook(() => useAppState(transferClient({ cherryPick, getStatus }), TEST_REPO_PATH));
    await act(() => result.current.cherryPick("c1"));
    expect(cherryPick).toHaveBeenCalledWith(TEST_REPO_PATH, "c1");
    expect(getStatus).toHaveBeenCalled();
    expect(result.current.state.error).toBeNull();
  });

  it("surfaces a backend refusal as state.error", async () => {
    const revertCommit = vi.fn(async () => {
      throw new Error("Cherry-picking or reverting merge commits isn't supported yet.");
    });
    const { result } = renderHook(() => useAppState(transferClient({ revertCommit }), TEST_REPO_PATH));
    await act(() => result.current.revertCommit("m1"));
    expect(result.current.state.error).toContain("merge commits isn't supported");
  });

  it("resetToCommit and checkoutCommit pass their arguments through", async () => {
    const resetToCommit = vi.fn(async () => {});
    const checkoutCommit = vi.fn(async () => {});
    const { result } = renderHook(() =>
      useAppState(transferClient({ resetToCommit, checkoutCommit }), TEST_REPO_PATH),
    );
    await act(() => result.current.resetToCommit("c1", "hard"));
    await act(() => result.current.checkoutCommit("c2"));
    expect(resetToCommit).toHaveBeenCalledWith(TEST_REPO_PATH, "c1", "hard");
    expect(checkoutCommit).toHaveBeenCalledWith(TEST_REPO_PATH, "c2");
  });

  it("amendCommit turns amend mode off on success and keeps it on failure", async () => {
    let fail = true;
    const amendCommit = vi.fn(async () => {
      if (fail) throw new Error("boom");
    });
    const { result } = renderHook(() => useAppState(transferClient({ amendCommit }), TEST_REPO_PATH));
    act(() => result.current.setAmendMode(true));
    await act(() => result.current.amendCommit("m"));
    expect(result.current.state.amendMode).toBe(true);
    fail = false;
    await act(() => result.current.amendCommit("m"));
    expect(result.current.state.amendMode).toBe(false);
  });
});
```

(If the file imports `renderHook`/`act`/`vi` under other names, use those.)

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/state/useAppState.test.ts -t "history actions"`
Expected: FAIL (`headInfo`/`cherryPick` undefined).

- [ ] **Step 3: Implement**

`frontend/src/state/useHistoryActions.ts`:

```ts
import { useCallback } from "react";
import type { RepoClient, ResetMode } from "../ipc/RepoClient";
import type { AppState } from "./useAppState";
import type { RunMutation } from "./useMutationRunner";

export interface HistoryActions {
  cherryPick(commitId: string): Promise<void>;
  revertCommit(commitId: string): Promise<void>;
  resetToCommit(commitId: string, mode: ResetMode): Promise<void>;
  checkoutCommit(commitId: string): Promise<void>;
  amendCommit(message: string): Promise<void>;
  setAmendMode(on: boolean): void;
}

// History-rewriting actions on a single commit. None are optimistic: each rewrites refs, and the
// conflict case (cherry-pick/revert) only becomes visible through the refresh that `runMutation`
// performs afterwards (MERGE_MSG ⇒ `mergeMessage`, CHERRY_PICK_HEAD ⇒ `headInfo.operation`).
export function useHistoryActions(
  client: RepoClient,
  repoPath: string,
  runMutation: RunMutation,
  setState: (updater: (prev: AppState) => AppState) => void,
): HistoryActions {
  const cherryPick = useCallback(
    (commitId: string) => runMutation(async () => void (await client.cherryPick(repoPath, commitId))),
    [client, runMutation, repoPath],
  );
  const revertCommit = useCallback(
    (commitId: string) => runMutation(async () => void (await client.revertCommit(repoPath, commitId))),
    [client, runMutation, repoPath],
  );
  const resetToCommit = useCallback(
    (commitId: string, mode: ResetMode) => runMutation(() => client.resetToCommit(repoPath, commitId, mode)),
    [client, runMutation, repoPath],
  );
  const checkoutCommit = useCallback(
    (commitId: string) => runMutation(() => client.checkoutCommit(repoPath, commitId)),
    [client, runMutation, repoPath],
  );
  const amendCommit = useCallback(
    (message: string) =>
      runMutation(async () => {
        await client.amendCommit(repoPath, message);
        setState((prev) => ({ ...prev, amendMode: false }));
      }),
    [client, runMutation, repoPath, setState],
  );
  const setAmendMode = useCallback(
    (on: boolean) => setState((prev) => ({ ...prev, amendMode: on })),
    [setState],
  );
  return { cherryPick, revertCommit, resetToCommit, checkoutCommit, amendCommit, setAmendMode };
}
```

In `useAppState.ts`:
- Add `headInfo: HeadInfo; amendMode: boolean;` to `AppState` and initial values `{ headId: null, detachedAt: null, headPushed: false, operation: null }` / `false` in the initial state object.
- In `refresh`, add `client.getHeadInfo(repoPath)` to the `Promise.all` tuple (append as last element and destructure as `headInfo`), and set `headInfo` in the `setState` call.
- `UseAppStateResult extends`/includes `HistoryActions`' members; call `useHistoryActions(client, repoPath, runMutation, setState)` next to `useMergeRebaseActions` and spread its result into the returned object (follow how `mergeRebase` actions are spread, lines ~421/485).

- [ ] **Step 4: Run tests**

Run: `cd frontend && pnpm exec tsc --noEmit && pnpm test -- --run src/state`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/state
git commit -m "feat(frontend): history actions and headInfo in app state"
```

---

### Task 10: Commit context menu + `ResetDialog`

**Files:**
- Create: `frontend/src/components/ResetDialog.tsx`, `ResetDialog.module.css`, `ResetDialog.test.tsx`
- Modify: `frontend/src/components/CommitGraph.tsx`, `CommitGraph.test.tsx`, `frontend/src/App.tsx`

**Interfaces:**
- Consumes: Task 9 actions; `HeadInfo`; `FormDialog`, `ConfirmDialog` primitives.
- Produces:
  ```ts
  // CommitGraph new props:
  headInfo: HeadInfo;
  currentBranch: string | null;              // from branches.find(b => b.isCurrent)?.name ?? null
  onCherryPick: (commitId: string) => void;
  onRevert: (commitId: string) => void;
  onCheckoutCommit: (commitId: string) => void;
  onResetToCommit: (commitId: string, mode: ResetMode) => void;
  onEditHeadMessage: () => void;
  // ResetDialog:
  export function ResetDialog(props: { branch: string; shortId: string; onReset: (mode: ResetMode) => void; onCancel: () => void }): JSX.Element
  ```
- Toast labels (via `announce` in `App.tsx`): "Cherry-picked", "Reverted", "Reset branch", "Checked out commit".

- [ ] **Step 1: Write the failing tests**

`ResetDialog.test.tsx`:

```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ResetDialog } from "./ResetDialog";

describe("ResetDialog", () => {
  it("defaults to mixed and resets with the chosen mode", () => {
    const onReset = vi.fn();
    render(<ResetDialog branch="main" shortId="abc1234" onReset={onReset} onCancel={() => {}} />);
    expect(screen.getByRole("radio", { name: /Mixed/ })).toBeChecked();
    fireEvent.click(screen.getByRole("radio", { name: /Soft/ }));
    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect(onReset).toHaveBeenCalledWith("soft");
  });

  it("asks for confirmation before a hard reset", () => {
    const onReset = vi.fn();
    render(<ResetDialog branch="main" shortId="abc1234" onReset={onReset} onCancel={() => {}} />);
    fireEvent.click(screen.getByRole("radio", { name: /Hard/ }));
    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect(onReset).not.toHaveBeenCalled();
    expect(screen.getByText("Discards all uncommitted changes. Cannot be undone.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Reset hard" }));
    expect(onReset).toHaveBeenCalledWith("hard");
  });

  it("names the branch and target", () => {
    render(<ResetDialog branch="main" shortId="abc1234" onReset={() => {}} onCancel={() => {}} />);
    expect(screen.getByText(/Reset main to abc1234/)).toBeInTheDocument();
  });
});
```

`CommitGraph.test.tsx` — new `describe` block, built like the existing "keyboard access" block (it reuses the file's module-level `commits` and `status` fixtures). Existing renders of `CommitGraph` in this file must also get the new required props: give them `headInfo={NO_HEAD}`, `currentBranch="main"` and `vi.fn()` for the five callbacks (add a shared `historyProps` object at the top and spread it into every existing `render(<CommitGraph …/>)` call and the `Host` / `setup` helpers).

```tsx
const NO_HEAD = { headId: null, detachedAt: null, headPushed: false, operation: null } as const;

describe("CommitGraph — history actions", () => {
  const chain: GraphCommit[] = [
    { ...commits[0], id: "C", shortId: "C", summary: "C", parentIds: ["B"] },
    { ...commits[0], id: "M", shortId: "M", summary: "M", parentIds: ["B", "A"] },
    { ...commits[0], id: "B", shortId: "B", summary: "B", parentIds: ["A"] },
    { ...commits[0], id: "A", shortId: "A", summary: "A", parentIds: [] },
  ];

  function setup(overrides: Partial<ComponentProps<typeof CommitGraph>> = {}) {
    const handlers = {
      onSelectRow: vi.fn(),
      onBranchFromCommit: vi.fn(),
      onRebaseFromCommit: vi.fn(),
      onRebaseSelection: vi.fn(),
      onCherryPick: vi.fn(),
      onRevert: vi.fn(),
      onCheckoutCommit: vi.fn(),
      onResetToCommit: vi.fn(),
      onEditHeadMessage: vi.fn(),
    };
    render(
      <CommitGraph
        status={status}
        commits={chain}
        selectedRow="uncommitted"
        pending={false}
        headInfo={{ ...NO_HEAD, headId: "C" }}
        currentBranch="main"
        {...handlers}
        {...overrides}
      />,
    );
    const openMenu = (summary: string) =>
      fireEvent.contextMenu(screen.getByText(summary, { selector: "span" }).closest("li")!);
    return { ...handlers, openMenu };
  }

  it("offers cherry-pick, revert, checkout and reset on a single commit", () => {
    const { openMenu, onCherryPick, onRevert, onCheckoutCommit, onResetToCommit } = setup();
    openMenu("B");
    fireEvent.click(screen.getByRole("menuitem", { name: "Cherry-pick" }));
    expect(onCherryPick).toHaveBeenCalledWith("B");
    openMenu("B");
    fireEvent.click(screen.getByRole("menuitem", { name: "Revert" }));
    expect(onRevert).toHaveBeenCalledWith("B");
    openMenu("B");
    fireEvent.click(screen.getByRole("menuitem", { name: "Checkout (detached)" }));
    expect(onCheckoutCommit).toHaveBeenCalledWith("B");
    openMenu("B");
    fireEvent.click(screen.getByRole("menuitem", { name: "Reset main to here…" }));
    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect(onResetToCommit).toHaveBeenCalledWith("B", "mixed");
  });

  it("disables cherry-pick on HEAD, offers Edit commit message there, and blocks merge commits", () => {
    const { openMenu, onEditHeadMessage } = setup();
    openMenu("C");
    expect(screen.getByRole("menuitem", { name: "Cherry-pick" })).toBeDisabled();
    fireEvent.click(screen.getByRole("menuitem", { name: "Edit commit message…" }));
    expect(onEditHeadMessage).toHaveBeenCalled();
    openMenu("M");
    expect(screen.getByRole("menuitem", { name: "Cherry-pick" })).toBeDisabled();
    expect(screen.getByRole("menuitem", { name: "Revert" })).toBeDisabled();
    expect(screen.queryByRole("menuitem", { name: "Edit commit message…" })).not.toBeInTheDocument();
  });

  it("disables the new actions while pending", () => {
    const { openMenu } = setup({ pending: true });
    openMenu("B");
    for (const name of ["Cherry-pick", "Revert", "Checkout (detached)", "Reset main to here…"]) {
      expect(screen.getByRole("menuitem", { name })).toBeDisabled();
    }
  });

  it("disables reset on a detached HEAD", () => {
    const { openMenu } = setup({ headInfo: { ...NO_HEAD, headId: "C", detachedAt: "C" }, currentBranch: null });
    openMenu("B");
    expect(screen.getByRole("menuitem", { name: "Reset HEAD to here…" })).toBeDisabled();
  });
});
```

(If `getByText(summary, { selector: "span" })` is ambiguous with the row's other spans, use the same lookup the existing "Rebase onto here" test uses at line ~795: `screen.getByText(/second commit/).closest("li")`, with an exact-match regex like `/^B$/`.)

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/components/ResetDialog.test.tsx src/components/CommitGraph.test.tsx`
Expected: FAIL.

- [ ] **Step 3: Implement**

`ResetDialog.tsx`:

```tsx
import { useState } from "react";
import type { ResetMode } from "../ipc/RepoClient";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import { FormDialog } from "./primitives/FormDialog";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./ResetDialog.module.css";

const MODES: Array<{ mode: ResetMode; label: string; detail: string }> = [
  { mode: "soft", label: "Soft", detail: "Keep all changes staged." },
  { mode: "mixed", label: "Mixed", detail: "Keep changes in the working tree, unstaged." },
  { mode: "hard", label: "Hard", detail: "Discard all changes to tracked files." },
];

export function ResetDialog({
  branch,
  shortId,
  onReset,
  onCancel,
}: {
  branch: string;
  shortId: string;
  onReset: (mode: ResetMode) => void;
  onCancel: () => void;
}) {
  const [mode, setMode] = useState<ResetMode>("mixed");
  const [confirmingHard, setConfirmingHard] = useState(false);

  if (confirmingHard) {
    return (
      <ConfirmDialog
        ariaLabel="Confirm hard reset"
        message={<p>Discards all uncommitted changes. Cannot be undone.</p>}
        confirmLabel="Reset hard"
        onConfirm={() => onReset("hard")}
        onCancel={() => setConfirmingHard(false)}
      />
    );
  }

  return (
    <FormDialog ariaLabel="Reset branch" onCancel={onCancel}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (mode === "hard") setConfirmingHard(true);
          else onReset(mode);
        }}
      >
        <p>
          Reset {branch} to {shortId}
        </p>
        <fieldset className={styles.modes}>
          <legend>Mode</legend>
          {MODES.map((option) => (
            <label key={option.mode} className={styles.mode}>
              <input
                type="radio"
                name="reset-mode"
                value={option.mode}
                checked={mode === option.mode}
                onChange={() => setMode(option.mode)}
              />
              <span>
                <strong>{option.label}</strong> — {option.detail}
              </span>
            </label>
          ))}
        </fieldset>
        <Toolbar>
          <button type="submit">Reset</button>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        </Toolbar>
      </form>
    </FormDialog>
  );
}
```

`ResetDialog.module.css`:

```css
.modes {
  border: none;
  padding: 0;
  margin: 0 0 var(--space-3, 12px);
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 8px);
}
.mode {
  display: flex;
  gap: var(--space-2, 8px);
  align-items: baseline;
}
```

(Use the spacing tokens other `.module.css` files use; check `FormDialog.module.css` for names.)

`CommitGraph.tsx`: add the new props; add `const [resetTarget, setResetTarget] = useState<GraphCommit | null>(null);`. Replace the single-commit `items` array with a helper:

```tsx
  function singleCommitItems(commitId: string): ContextMenuItem[] {
    const commit = commits.find((candidate) => candidate.id === commitId);
    const isHead = headInfo.headId === commitId;
    const isMerge = (commit?.parentIds.length ?? 0) > 1;
    const items: ContextMenuItem[] = [
      { label: "Branch from here", onSelect: () => onBranchFromCommit(commitId) },
      { label: "Rebase onto here", onSelect: () => onRebaseFromCommit(commitId), disabled: pending },
      { label: "Cherry-pick", onSelect: () => onCherryPick(commitId), disabled: pending || isHead || isMerge },
      { label: "Revert", onSelect: () => onRevert(commitId), disabled: pending || isMerge },
      { label: "Checkout (detached)", onSelect: () => onCheckoutCommit(commitId), disabled: pending },
      {
        label: `Reset ${currentBranch ?? "HEAD"} to here…`,
        onSelect: () => setResetTarget(commit ?? null),
        disabled: pending || currentBranch === null,
      },
    ];
    if (isHead) {
      items.push({ label: "Edit commit message…", onSelect: onEditHeadMessage, disabled: pending });
    }
    return items;
  }
```

and use `singleCommitItems(contextMenu.commitId)` where the inline array was. Render after the context menu:

```tsx
      {resetTarget !== null && currentBranch !== null && (
        <ResetDialog
          branch={currentBranch}
          shortId={resetTarget.shortId}
          onReset={(mode) => {
            onResetToCommit(resetTarget.id, mode);
            setResetTarget(null);
          }}
          onCancel={() => setResetTarget(null)}
        />
      )}
```

(`ResetDialog` renders a `<dialog>`; if placing it inside the `<ul>` is invalid DOM, wrap the component's return in a fragment and render the dialog as a sibling of the `<ul>`.)

`App.tsx` `<CommitGraph …>`: pass

```tsx
                headInfo={appState.state.headInfo}
                currentBranch={appState.state.branches.find((branch) => branch.isCurrent)?.name ?? null}
                onCherryPick={announce("Cherry-picked", appState.cherryPick)}
                onRevert={announce("Reverted", appState.revertCommit)}
                onCheckoutCommit={announce("Checked out commit", appState.checkoutCommit)}
                onResetToCommit={announce("Reset branch", appState.resetToCommit)}
                onEditHeadMessage={() => appState.setAmendMode(true)}
```

Also update `operationDisabledReason` in `App.tsx` so the merge branch reads from `headInfo.operation` when present:

```ts
      : appState.state.mergeMessage !== null
        ? appState.state.headInfo.operation === "CherryPick"
          ? "A cherry-pick is in progress."
          : appState.state.headInfo.operation === "Revert"
            ? "A revert is in progress."
            : "A merge is in progress."
```

Note: a conflicted cherry-pick shows the "Cherry-picked" toast only if `state.error` is null — that is correct (the operation started); the status strip (Task 12) then shows the pause.

- [ ] **Step 4: Run tests**

Run: `cd frontend && pnpm exec tsc --noEmit && pnpm lint && pnpm test -- --run`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): cherry-pick, revert, detached checkout and reset in the commit menu"
```

---

### Task 11: CommitBox amend toggle

**Files:**
- Modify: `frontend/src/components/CommitBox.tsx`, `CommitBox.test.tsx`, `frontend/src/components/DiffPane.tsx` (prop threading), `frontend/src/App.tsx`

**Interfaces:**
- Consumes: `state.amendMode`, `setAmendMode`, `amendCommit`, `headInfo` (Task 9); `client.getCommitMessage(repoPath, headId)` (existing).
- Produces — CommitBox new props:
  ```ts
  amend?: {
    available: boolean;           // headId !== null && no operation in progress
    on: boolean;
    headMessage: string | null;   // loaded by DiffPane when `on` flips true; null while loading
    pushed: boolean;
    onToggle: (on: boolean) => void;
    onAmend: (message: string) => void;
  };
  ```

- [ ] **Step 1: Write the failing tests** (append to `CommitBox.test.tsx`, using its existing render pattern)

```tsx
describe("amend", () => {
  function renderBox(overrides: Partial<NonNullable<Parameters<typeof CommitBox>[0]["amend"]>> = {}, draft?: string) {
    const amend = {
      available: true,
      on: false,
      headMessage: "previous message",
      pushed: false,
      onToggle: vi.fn(),
      onAmend: vi.fn(),
      ...overrides,
    };
    const onCommit = vi.fn();
    const utils = render(
      <CommitBox
        onCommit={onCommit}
        disabled={true}
        disabledReason="Stage changes to commit"
        onAbortMerge={() => {}}
        initialDraft={draft === undefined ? undefined : { message: draft, lastSeeded: "" }}
        amend={amend}
      />,
    );
    return { ...utils, amend, onCommit };
  }

  it("shows the toggle and calls onToggle", () => {
    const { amend } = renderBox();
    fireEvent.click(screen.getByRole("checkbox", { name: "Amend last commit" }));
    expect(amend.onToggle).toHaveBeenCalledWith(true);
  });

  it("prefills HEAD's message, relabels the button and allows an empty index", () => {
    const { amend, rerender } = renderBox({ on: true });
    expect(screen.getByLabelText("Commit message")).toHaveValue("previous message");
    const button = screen.getByRole("button", { name: "Amend" });
    expect(button).toBeEnabled();
    fireEvent.click(button);
    expect(amend.onAmend).toHaveBeenCalledWith("previous message");
    void rerender;
  });

  it("restores the typed draft exactly when toggled off", () => {
    const props = { onCommit: vi.fn(), disabled: true, onAbortMerge: () => {} };
    const amend = { available: true, headMessage: "previous message", pushed: false, onToggle: vi.fn(), onAmend: vi.fn() };
    const { rerender } = render(<CommitBox {...props} amend={{ ...amend, on: false }} />);
    fireEvent.change(screen.getByLabelText("Commit message"), { target: { value: "half-typed\nwork" } });
    rerender(<CommitBox {...props} amend={{ ...amend, on: true }} />);
    expect(screen.getByLabelText("Commit message")).toHaveValue("previous message");
    rerender(<CommitBox {...props} amend={{ ...amend, on: false }} />);
    expect(screen.getByLabelText("Commit message")).toHaveValue("half-typed\nwork");
  });

  it("warns when HEAD is already pushed", () => {
    renderBox({ on: true, pushed: true });
    expect(screen.getByText("Already pushed — you'll need to force push.")).toBeInTheDocument();
  });

  it("hides the toggle when amend is unavailable", () => {
    renderBox({ available: false });
    expect(screen.queryByRole("checkbox", { name: "Amend last commit" })).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/components/CommitBox.test.tsx`
Expected: FAIL.

- [ ] **Step 3: Implement**

In `CommitBox.tsx`:
- Accept `amend` prop.
- Add `const [stashedDraft, setStashedDraft] = useState<string | null>(null);`.
- Effect keyed on `amend?.on` and `amend?.headMessage`:

```tsx
  const amendOn = amend?.on === true;
  const headMessage = amend?.headMessage ?? null;
  useEffect(() => {
    if (amendOn && stashedDraft === null && headMessage !== null) {
      // Deliberate seed write: entering amend mode swaps the draft out for HEAD's message.
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setStashedDraft(message);
      setMessage(headMessage);
    } else if (!amendOn && stashedDraft !== null) {
      setMessage(stashedDraft);
      setStashedDraft(null);
    }
  }, [amendOn, headMessage, stashedDraft, message]);
```

- `commitIfReady`: if `amendOn`, require only `message.trim() !== ""` (ignore `disabled`), call `amend.onAmend(message)`, then `setStashedDraft(null); setMessage(""); setLastSeeded("");`. Otherwise existing behavior.
- Render, before `<Toolbar>`:

```tsx
      {amend?.available === true && (
        <label className={styles.amendToggle}>
          <input type="checkbox" checked={amendOn} onChange={(event) => amend.onToggle(event.target.checked)} />
          Amend last commit
        </label>
      )}
      {amendOn && amend?.pushed === true && (
        <p className={styles.reason}>Already pushed — you'll need to force push.</p>
      )}
```

- Button: label `{amendOn ? "Amend" : "Commit"}`; `disabled={amendOn ? message.trim() === "" : disabled || message.trim() === ""}`; hide the `disabledReason` paragraph while `amendOn`.
- Add `.amendToggle { display: flex; gap: var(--space-1, 4px); align-items: center; }` to `CommitBox.module.css`.

In `DiffPane.tsx`: accept `amendMode: boolean`, `headInfo: HeadInfo`, `onSetAmendMode`, `onAmendCommit` props and load HEAD's message when amend turns on:

```tsx
  const [headMessage, setHeadMessage] = useState<string | null>(null);
  useEffect(() => {
    if (!amendMode || headInfo.headId === null) {
      setHeadMessage(null);
      return;
    }
    let cancelled = false;
    void client.getCommitMessage(repoPath, headInfo.headId).then(
      (text) => !cancelled && setHeadMessage(text),
      () => !cancelled && setHeadMessage(""),
    );
    return () => {
      cancelled = true;
    };
  }, [amendMode, headInfo.headId, client, repoPath]);
```

and pass to `<CommitBox>`:

```tsx
            amend={{
              available: headInfo.headId !== null && headInfo.operation === null && mergeMessage === null,
              on: amendMode,
              headMessage,
              pushed: headInfo.headPushed,
              onToggle: onSetAmendMode,
              onAmend: onAmendCommit,
            }}
```

(Check `getCommitMessage`'s exact signature in `RepoClient.ts` before writing; thread the props through both `DiffPane` inner components the same way `onAbortMerge` is threaded at lines 328–461.)

In `App.tsx` `<DiffPane …>` pass `amendMode={appState.state.amendMode}`, `headInfo={appState.state.headInfo}`, `onSetAmendMode={appState.setAmendMode}`, `onAmendCommit={announce("Amended commit", appState.amendCommit)}`. When "Edit commit message…" runs (`onEditHeadMessage`), also select the uncommitted row so the CommitBox is visible: `() => { appState.selectRow("uncommitted"); appState.setAmendMode(true); }`.

- [ ] **Step 4: Run tests**

Run: `cd frontend && pnpm exec tsc --noEmit && pnpm lint && pnpm test -- --run`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): amend last commit toggle and edit-message entry point"
```

---

### Task 12: Operation strip, detached HEAD display, force-push offer

**Files:**
- Modify: `frontend/src/components/OperationStatusStrip.tsx` (+ test), `frontend/src/components/SyncBar.tsx` (+ test), `frontend/src/components/CommitBox.tsx` (abort label), `frontend/src/state/useRemoteTransferActions.ts`, `frontend/src/state/useAppState.ts`, `frontend/src/state/useAppState.test.ts`, `frontend/src/App.tsx`

**Interfaces:**
- Consumes: `headInfo` (Task 9), `forcePushCurrentBranch` (Task 8).
- Produces:
  ```ts
  // OperationStatusStrip new prop:
  operation: OperationKind | null;    // replaces the merge label choice
  // AppState gains:
  forcePushOffer: { remoteName: string } | null;     // set when a PushBranch fails NonFastForward; shows the offer button
  forcePushConfirm: { remoteName: string } | null;   // non-null ⇒ App renders the confirm dialog
  // actions gain:
  requestForcePush(remoteName: string): void;        // opens the confirm (offer button + palette both use this)
  cancelForcePush(): void;                           // clears offer + confirm
  forcePushCurrentBranch(remoteName: string): Promise<void>;  // clears offer + confirm, starts the transfer
  ```

- [ ] **Step 1: Write the failing tests**

`OperationStatusStrip.test.tsx` (follow existing render style):

```tsx
  it.each([
    ["CherryPick", "Cherry-picking", "Abort cherry-pick"],
    ["Revert", "Reverting", "Abort revert"],
    ["Merge", "Merging", "Abort merge"],
  ] as const)("labels a paused %s", (operation, label, abort) => {
    const onAbortMerge = vi.fn();
    render(
      <OperationStatusStrip
        merging
        operation={operation}
        rebaseProgress={null}
        conflictCount={1}
        onAbortMerge={onAbortMerge}
        onAbortRebase={() => {}}
      />,
    );
    expect(screen.getByText(label)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: abort }));
    expect(onAbortMerge).toHaveBeenCalled();
  });
```

`useAppState.test.ts` — following the existing transfer-failure tests at lines ~570–600 (they capture the `subscribeTransferProgress` listener):

```ts
  it("offers a force push after a non-fast-forward branch push and runs it", async () => {
    let listener: ((progress: TransferProgress) => void) | null = null;
    const forcePushCurrentBranch = vi.fn(async () => "force-push-1");
    const client = transferClient({
      pushCurrentBranch: async () => "push-1",
      forcePushCurrentBranch,
      subscribeTransferProgress: (callback) => {
        listener = callback;
        return () => {};
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.pushCurrentBranch("origin"));
    act(() => listener!({ operationId: "push-1", operation: "PushBranch", phase: "Failed", errorKind: "NonFastForward", current: 0, total: 0, receivedBytes: 0, message: null }));
    expect(result.current.state.forcePushOffer).toEqual({ remoteName: "origin" });

    act(() => result.current.requestForcePush("origin"));
    expect(result.current.state.forcePushConfirm).toEqual({ remoteName: "origin" });

    await act(() => result.current.forcePushCurrentBranch("origin"));
    expect(forcePushCurrentBranch).toHaveBeenCalledWith(TEST_REPO_PATH, "origin");
    expect(result.current.state.forcePushOffer).toBeNull();
    expect(result.current.state.forcePushConfirm).toBeNull();
  });

  it("cancelForcePush clears both the offer and the confirm", () => {
    const { result } = renderHook(() => useAppState(transferClient({}), TEST_REPO_PATH));
    act(() => result.current.requestForcePush("origin"));
    act(() => result.current.cancelForcePush());
    expect(result.current.state.forcePushConfirm).toBeNull();
    expect(result.current.state.forcePushOffer).toBeNull();
  });

  it("explains a stale lease", async () => {
    let listener: ((progress: TransferProgress) => void) | null = null;
    const client = transferClient({
      forcePushCurrentBranch: async () => "force-push-1",
      getCurrentUpstream: async () => ({ localBranch: "main", remoteName: "origin", remoteBranch: "main" }),
      subscribeTransferProgress: (callback) => {
        listener = callback;
        return () => {};
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.forcePushCurrentBranch("origin"));
    act(() => listener!({ operationId: "force-push-1", operation: "ForcePushBranch", phase: "Failed", errorKind: "StaleLease", current: 0, total: 0, receivedBytes: 0, message: null }));
    expect(result.current.state.error).toBe(
      "origin/main changed since your last fetch. Fetch and review before force pushing.",
    );
  });
```

(Match the `TransferProgress` field names to `RepoClient.ts` exactly.)

`SyncBar.test.tsx`:

```tsx
  it("shows a detached HEAD and disables push and pull", () => {
    /* render SyncBar with the existing helper plus detachedAt="abc1234" */
    expect(screen.getByText("HEAD detached at abc1234")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Push/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: /Pull/ })).toBeDisabled();
  });
```

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/components/OperationStatusStrip.test.tsx src/components/SyncBar.test.tsx src/state/useAppState.test.ts`
Expected: FAIL.

- [ ] **Step 3: Implement**

`OperationStatusStrip.tsx`: add `operation: OperationKind | null` prop;

```tsx
  const mergeLabel =
    operation === "CherryPick" ? "Cherry-picking" : operation === "Revert" ? "Reverting" : "Merging";
  const abortLabel =
    operation === "CherryPick" ? "Abort cherry-pick" : operation === "Revert" ? "Abort revert" : "Abort merge";
```

use `mergeLabel` in place of `"Merging"` and `abortLabel` in place of `"Abort merge"`. Update the doc comment ("merge, cherry-pick, revert or rebase"). In `App.tsx`, pass `operation={appState.state.headInfo.operation}`.

`CommitBox.tsx`: accept optional `abortLabel?: string` (default "Abort merge") for the existing abort button; `DiffPane` passes the same `abortLabel` derived from `headInfo.operation`.

`useRemoteTransferActions.ts`:
- In `transferFailureMessage`, treat `ForcePushBranch` as a push (`isPush` includes it). Add a `upstreamLabel` parameter: `transferFailureMessage(progress, upstreamLabel: string)` and

```ts
  if (progress.errorKind === "StaleLease") {
    return `${upstreamLabel} changed since your last fetch. Fetch and review before force pushing.`;
  }
```

  where the subscriber computes `upstreamLabel` from a ref mirroring `state.upstream` (`${remoteName}/${remoteBranch}`, falling back to the remote name). Keep a `useRef` updated from the state's upstream (`upstreamRef.current = …` via `setState` callers is awkward — instead accept `upstream: UpstreamInfo | null` as a hook argument and mirror it into a ref with `useEffect`; `useAppState` passes `state.upstream`).
- Track the remote of the last branch push in a `lastPushRemote` ref (set in `pushCurrentBranch`). In the `Failed` branch:

```ts
        const offer =
          progress.operation === "PushBranch" && progress.errorKind === "NonFastForward" && lastPushRemote.current !== null
            ? { remoteName: lastPushRemote.current }
            : null;
        setState((prev) => ({ ...prev, transfer: null, error: transferFailureMessage(progress, upstreamLabel()), pending: false, forcePushOffer: offer }));
```

- Add:

```ts
  const requestForcePush = useCallback(
    (remoteName: string) => setState((prev) => ({ ...prev, forcePushConfirm: { remoteName } })),
    [setState],
  );
  const cancelForcePush = useCallback(
    () => setState((prev) => ({ ...prev, forcePushOffer: null, forcePushConfirm: null })),
    [setState],
  );
  const forcePushCurrentBranch = useCallback(
    (remoteName: string) => {
      setState((prev) => ({ ...prev, forcePushOffer: null, forcePushConfirm: null }));
      return startTransfer("ForcePushBranch", () => client.forcePushCurrentBranch(repoPath, remoteName));
    },
    [client, startTransfer, repoPath, setState],
  );
```

  and add all three to `RemoteTransferActions` and the returned object. `AppState` gains `forcePushOffer` and `forcePushConfirm` (both initial `null`). Add both fields (and the new actions as `vi.fn()`) to `makeAppState` in `lib/commands.test.ts` and any other full `AppState`/`UseAppStateResult` fixture `tsc` flags; do the same for `headInfo`/`amendMode` and the Task 9 actions if Task 9 didn't already.

`App.tsx`: render next to the existing `state.error` display (find the error banner / `InlineError` in `App.tsx`; put the button inside or right after it):

```tsx
      {appState.state.forcePushOffer !== null && appState.state.forcePushConfirm === null && (
        <button type="button" onClick={() => appState.requestForcePush(appState.state.forcePushOffer!.remoteName)}>
          Force push (with lease)…
        </button>
      )}
      {appState.state.forcePushConfirm !== null && (
        <ConfirmDialog
          ariaLabel="Confirm force push"
          message={
            <p>
              Overwrites {appState.state.forcePushConfirm.remoteName}/{currentBranchName} with your local branch.
            </p>
          }
          confirmLabel="Force push"
          onConfirm={() =>
            void announce("Force pushed", appState.forcePushCurrentBranch)(appState.state.forcePushConfirm!.remoteName)
          }
          onCancel={appState.cancelForcePush}
        />
      )}
```

with `const currentBranchName = appState.state.branches.find((branch) => branch.isCurrent)?.name ?? "";`.

Add an App-level test in `App.test.tsx` only if `App.test.tsx` already covers the error banner; otherwise the hook test above is the coverage.

`SyncBar.tsx`: accept `detachedAt: string | null`; when non-null render `<span>HEAD detached at {detachedAt}</span>` in place of the branch/ahead-behind label, and disable Push/Pull with `title="Check out a branch to push or pull."`. Pass `detachedAt={appState.state.headInfo.detachedAt}` from `App.tsx`.

Branch tree current marker: if `BranchTree` shows nothing when no branch `isCurrent`, that is acceptable — the SyncBar label covers the spec's "sidebar … show HEAD detached". If `BranchTree` has a header line naming the current branch, render the same text there.

- [ ] **Step 4: Run tests**

Run: `cd frontend && pnpm exec tsc --noEmit && pnpm lint && pnpm test -- --run`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): operation labels for cherry-pick/revert, detached HEAD display, force push with lease offer"
```

---

### Task 13: Command palette

**Files:**
- Modify: `frontend/src/lib/commands.ts`, `frontend/src/lib/commands.test.ts`, `frontend/src/App.tsx` (only if `buildCommands` needs a new callback)

**Interfaces:**
- Consumes: Task 9/12 actions; `state.selectedRow`, `state.headInfo`, `state.commits`, `state.upstream`.
- Produces command ids: `cherry-pick-selected`, `revert-selected`, `checkout-selected`, `reset-selected`, `amend-last-commit`, `force-push`.

- [ ] **Step 1: Write the failing tests** (append to `commands.test.ts`; `makeAppState(stateOverrides)` is the file's existing fixture — it takes state overrides only, so action spies are spread over its result)

```ts
describe("history commands", () => {
  const HISTORY_IDS = [
    "cherry-pick-selected",
    "revert-selected",
    "checkout-selected",
    "reset-selected",
    "amend-last-commit",
    "force-push",
  ];
  const graphCommit = (id: string, parentIds: string[]) => ({
    id,
    shortId: id,
    summary: `commit ${id}`,
    authorName: "Test",
    authorEmail: "test@example.com",
    timestamp: 0,
    parentIds,
    branchRefs: [],
    remoteBranchRefs: [],
  });
  const historyState = {
    selectedRow: { commitId: "c2" },
    commits: [graphCommit("c1", ["c2"]), graphCommit("c2", [])],
    headInfo: { headId: "c1", detachedAt: null, headPushed: false, operation: null },
  } as const;

  it("offers the history commands when a commit is selected and nothing is running", () => {
    const ids = buildCommands(makeAppState(historyState)).map((command) => command.id);
    expect(ids).toEqual(expect.arrayContaining(HISTORY_IDS));
  });

  it("omits them while a merge is in progress", () => {
    const ids = buildCommands(makeAppState({ ...historyState, mergeMessage: "Merge x" })).map((c) => c.id);
    for (const id of HISTORY_IDS) expect(ids).not.toContain(id);
  });

  it("omits cherry-pick for HEAD and force push on a detached HEAD", () => {
    const ids = buildCommands(
      makeAppState({
        ...historyState,
        selectedRow: { commitId: "c1" },
        headInfo: { headId: "c1", detachedAt: "c1", headPushed: false, operation: null },
      }),
    ).map((c) => c.id);
    expect(ids).not.toContain("cherry-pick-selected");
    expect(ids).not.toContain("force-push");
  });

  it("runs cherry-pick on the selected commit and force push through the confirm", () => {
    const cherryPick = vi.fn();
    const requestForcePush = vi.fn();
    const appState = { ...makeAppState(historyState), cherryPick, requestForcePush };
    const commands = buildCommands(appState);
    commands.find((command) => command.id === "cherry-pick-selected")!.run();
    commands.find((command) => command.id === "force-push")!.run();
    expect(cherryPick).toHaveBeenCalledWith("c2");
    expect(requestForcePush).toHaveBeenCalledWith("origin");
  });

  it("groups force push under Remotes", () => {
    expect(commandGroup("force-push")).toBe("Remotes");
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/lib/commands.test.ts -t "history commands"`
Expected: FAIL.

- [ ] **Step 3: Implement**

In `buildCommands`, inside the existing `if (!repositoryOperationDisabled) { … }` block:

```ts
    const selected = state.selectedRow === "uncommitted" ? null : state.selectedRow.commitId;
    const selectedCommit = selected === null ? undefined : state.commits.find((c) => c.id === selected);
    if (selectedCommit !== undefined) {
      const isMerge = selectedCommit.parentIds.length > 1;
      const isHead = state.headInfo.headId === selectedCommit.id;
      if (!isMerge && !isHead) {
        commands.push({
          id: "cherry-pick-selected",
          label: `Cherry-pick ${selectedCommit.shortId}`,
          keywords: ["cherry", "pick", "apply", "commit"],
          run: () => void appState.cherryPick(selectedCommit.id),
        });
      }
      if (!isMerge) {
        commands.push({
          id: "revert-selected",
          label: `Revert ${selectedCommit.shortId}`,
          keywords: ["revert", "undo", "commit"],
          run: () => void appState.revertCommit(selectedCommit.id),
        });
      }
      commands.push({
        id: "checkout-selected",
        label: `Checkout ${selectedCommit.shortId} (detached)`,
        keywords: ["checkout", "detach", "commit"],
        run: () => void appState.checkoutCommit(selectedCommit.id),
      });
      commands.push({
        id: "reset-selected",
        label: `Reset to ${selectedCommit.shortId}…`,
        keywords: ["reset", "soft", "mixed", "hard"],
        run: () => goToCommitMenu(selectedCommit.id),
      });
    }
    if (state.headInfo.headId !== null) {
      commands.push({
        id: "amend-last-commit",
        label: "Amend last commit",
        keywords: ["amend", "edit", "message", "commit"],
        run: () => {
          appState.selectRow("uncommitted");
          appState.setAmendMode(true);
        },
      });
    }
    if (state.upstream !== null && state.headInfo.detachedAt === null) {
      const remoteName = state.upstream.remoteName;
      commands.push({
        id: "force-push",
        label: "Force push current branch…",
        keywords: ["force", "push", "lease"],
        run: () => appState.requestForcePush(remoteName),
      });
    }
```

- "Reset to …" must reach the mode dialog, not reset directly. Follow the file's existing "navigate to where the real UI lives" pattern (`expandRemoteBranches`): define `goToCommitMenu(commitId)` that finds the commit row (`document.querySelector(`[data-active-repo="true"] [data-commit-id="${commitId}"]`)`) and dispatches a `contextmenu` event on it. Add `data-commit-id={commit.id}` to the commit `ListRow` in `CommitGraph.tsx` if not already present.
- "Force push current branch…" opens the same confirm dialog as the rejection offer via `requestForcePush` (Task 12).
- `commandGroup`: add `id === "force-push"` to the "Remotes" group.

- [ ] **Step 4: Run tests**

Run: `cd frontend && pnpm exec tsc --noEmit && pnpm lint && pnpm test -- --run`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): palette commands for cherry-pick, revert, checkout, reset, amend and force push"
```

---

### Task 14: E2E flows

**Files:**
- Create: `e2e/specs/history-rewrite.spec.ts`, `extension/e2e/src/specs/cherry-pick.spec.ts`
- Modify: `extension/e2e/src/support/webviewPage.ts` (add `contextMenuByText`)

**Interfaces:**
- Consumes: UI labels from Tasks 10–12 ("Cherry-pick", "Revert", "Reset main to here…", "Reset", "Reset hard", toast texts).

- [ ] **Step 1: Write the Tauri spec** (pattern: `e2e/specs/merge.spec.ts` — git CLI fixtures in `before`, synthetic `contextmenu` events, `browser.execute` clicks)

```ts
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const git = (...args: string[]) =>
  execFileSync("git", args, { cwd: E2E_REPO_PATH }).toString().trim();

async function openCommitMenu(summary: string) {
  const row = await $(`li*=${summary}`);
  await row.waitForExist({ timeout: 10000 });
  await browser.execute((el) => {
    el.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }));
  }, row);
}

async function clickMenuItem(label: string) {
  const item = await $(`button*=${label}`);
  await item.waitForExist({ timeout: 10000 });
  await browser.execute((el) => (el as HTMLElement).click(), item);
}

describe("Browsitory history rewrite", () => {
  let baseBranch = "";
  let startTip = "";

  before(() => {
    baseBranch = git("rev-parse", "--abbrev-ref", "HEAD");
    git("checkout", "-b", "e2e-pick-source");
    fs.writeFileSync(path.join(E2E_REPO_PATH, "picked.txt"), "picked\n");
    git("add", "picked.txt");
    git("commit", "-m", "e2e: commit to cherry-pick");
    git("checkout", baseBranch);
    startTip = git("rev-parse", "HEAD");
  });

  it("cherry-picks a commit, reverts it, then resets hard back", async () => {
    await openCommitMenu("e2e: commit to cherry-pick");
    await clickMenuItem("Cherry-pick");
    await browser.waitUntil(() => fs.existsSync(path.join(E2E_REPO_PATH, "picked.txt")), { timeout: 10000 });
    expect(git("log", "-1", "--format=%s")).toBe("e2e: commit to cherry-pick");

    // The picked copy is now HEAD on baseBranch; it is the first matching row.
    await openCommitMenu("e2e: commit to cherry-pick");
    await clickMenuItem("Revert");
    await browser.waitUntil(() => !fs.existsSync(path.join(E2E_REPO_PATH, "picked.txt")), { timeout: 10000 });
    expect(git("log", "-1", "--format=%s")).toBe('Revert "e2e: commit to cherry-pick"');

    const startSummary = execFileSync("git", ["log", "-1", "--format=%s", startTip], { cwd: E2E_REPO_PATH }).toString().trim();
    await openCommitMenu(startSummary);
    await clickMenuItem(`Reset ${baseBranch} to here…`);
    const hard = await $("input[type=radio][value=hard]");
    await browser.execute((el) => (el as HTMLElement).click(), hard);
    await clickMenuItem("Reset");
    await clickMenuItem("Reset hard");
    await browser.waitUntil(() => git("rev-parse", "HEAD") === startTip, { timeout: 10000 });
  });
});
```

(If `openCommitMenu("e2e: commit to cherry-pick")` matches the source-branch row first, target the row by position instead — inspect the graph's row order in the running app and adjust; do not weaken the assertions.)

- [ ] **Step 2: Run the Tauri E2E** (build steps are order-sensitive — run exactly as `CLAUDE.md` lists)

```bash
cd frontend && VITE_E2E_REPO_PATH=/tmp/browsitory-e2e-repo pnpm build && cd ..
cargo build --workspace --features tauri-app/custom-protocol,tauri-app/forge-fixture-override
cd e2e && pnpm install && pnpm test -- --spec specs/history-rewrite.spec.ts
```

Expected: PASS. (Headless: prefix `pnpm test` with `xvfb-run -a`.)

- [ ] **Step 3: Write the VSCode smoke spec**

First add a context-menu helper to `extension/e2e/src/support/webviewPage.ts`, next to `clickByText`:

```ts
  async contextMenuByText(tag: string, text: string): Promise<void> {
    await this.evaluate(`
      (function () {
        var els = Array.from(document.querySelectorAll(${JSON.stringify(tag)}));
        var el = els.find(function (candidate) {
          return candidate.textContent && candidate.textContent.indexOf(${JSON.stringify(text)}) !== -1;
        });
        if (!el) throw new Error(${JSON.stringify(`no <${tag}> containing ${text}`)});
        el.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }));
      })()
    `);
  }
```

Then `extension/e2e/src/specs/cherry-pick.spec.ts`:

```ts
import { execFileSync } from "node:child_process";
import * as vscode from "vscode";
import { connectToWebview } from "../support/webviewPage";

describe("Browsitory VSCode extension cherry-pick", () => {
  it("cherry-picks a commit from another branch via the commit menu", async function () {
    // Same budget rationale as first-flow.spec.ts (webview iframe can take ~24s to appear).
    this.timeout(120000);

    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (!workspaceFolder) throw new Error("no workspace folder open");
    const cwd = workspaceFolder.uri.fsPath;
    const git = (...args: string[]) => execFileSync("git", args, { cwd }).toString().trim();

    const baseBranch = git("rev-parse", "--abbrev-ref", "HEAD");
    git("checkout", "-b", "vscode-e2e-pick-source");
    await vscode.workspace.fs.writeFile(
      vscode.Uri.joinPath(workspaceFolder.uri, "vscode-picked.txt"),
      Buffer.from("picked\n"),
    );
    git("add", "vscode-picked.txt");
    git("commit", "-m", "vscode-e2e: commit to cherry-pick");
    git("checkout", baseBranch);

    await vscode.commands.executeCommand("browsitory.open");
    const cdpPort = process.env["BROWSITORY_VSCODE_E2E_CDP_PORT"];
    if (!cdpPort) throw new Error("BROWSITORY_VSCODE_E2E_CDP_PORT not set");
    const webview = await connectToWebview(`http://127.0.0.1:${cdpPort}`);

    try {
      await webview.waitForText("li", "vscode-e2e: commit to cherry-pick", 15000);
      await webview.contextMenuByText("li", "vscode-e2e: commit to cherry-pick");
      await webview.clickByText("button", "Cherry-pick");

      const deadline = Date.now() + 15000;
      while (git("log", "-1", "--format=%s") !== "vscode-e2e: commit to cherry-pick") {
        if (Date.now() > deadline) throw new Error("cherry-pick did not land on the base branch");
        await new Promise((resolve) => setTimeout(resolve, 250));
      }
    } finally {
      await webview.close();
    }
  });
});
```

(If menu items render as `role="menuitem"` elements other than `<button>`, change the `clickByText` tag to match — check `ContextMenu.tsx`. If the suite runner only loads specs listed in `extension/e2e/src/suite/index.ts`, add this file there.)

- [ ] **Step 4: Run the VSCode E2E** (exact `CLAUDE.md` sequence)

```bash
cargo build --workspace
cd frontend && pnpm run generate:release-notes && VITE_E2E_REPO_PATH=/tmp/browsitory-vscode-e2e-repo pnpm exec vite build --config vite.vscode.config.ts && cd ..
cd extension && pnpm install && pnpm run compile && cd ..
cd extension/e2e && pnpm install --ignore-workspace && pnpm test
```

Expected: PASS (first-flow + cherry-pick).

- [ ] **Step 5: Commit**

```bash
git add e2e/specs/history-rewrite.spec.ts extension/e2e/src/specs/cherry-pick.spec.ts extension/e2e/src/support/webviewPage.ts
git commit -m "test(e2e): cherry-pick, revert and hard reset flow; VSCode cherry-pick smoke"
```

---

### Task 15: Docs and changelog

**Files:**
- Modify: `docs/USER_GUIDE.md`, `CHANGELOG.md`

- [ ] **Step 1: User guide** — add a section after the interactive-rebase section:

```markdown
## Rewriting and moving commits

Right-click a commit in the graph:

- **Cherry-pick** applies that commit on top of your current branch. If it conflicts, the
  status strip shows **Cherry-picking**, conflicts resolve exactly like a merge, and **Commit**
  finishes it (**Abort cherry-pick** undoes it). Merge commits can't be cherry-picked yet.
- **Revert** commits the inverse of that commit, with the same conflict flow.
- **Checkout (detached)** checks the commit out without a branch. The sync bar shows
  **HEAD detached at …**; push and pull are disabled until you check out a branch.
- **Reset ⟨branch⟩ to here…** moves your branch: **Soft** keeps your changes staged, **Mixed**
  keeps them unstaged, **Hard** discards changes to tracked files (untracked files are kept).
  Hard asks for confirmation.

**Amend last commit**: tick the checkbox under the commit message. The message fills with the
last commit's; **Amend** folds anything staged into it (or just rewords it). **Edit commit
message…** on the latest commit's menu does the same. If that commit was already pushed you'll
need to force push.

**Force push**: when a push is rejected because the remote has commits you don't, the error
offers **Force push (with lease)…**; the palette also has **Force push current branch…**. The
push only goes through if the remote branch is still where it was at your last fetch —
otherwise fetch, review, and try again. (Like `git push --force-with-lease`, there is a very
short window between that check and the remote applying the update.)
```

- [ ] **Step 2: Changelog** — under `## [Unreleased]`, add a new `### Cherry-pick, revert, reset, amend and force push` block above the multi-select block:

```markdown
### Cherry-pick, revert, reset, amend and force push
- Commit context menu: Cherry-pick, Revert, Checkout (detached) and "Reset ⟨branch⟩ to here…"
  (soft, mixed or hard; hard asks first). Cherry-pick and revert conflicts use the merge
  conflict flow, with their own status-strip label and abort button.
- "Amend last commit" toggle in the commit box, plus "Edit commit message…" on the latest
  commit; warns when that commit is already pushed.
- Force push with lease: offered when a push is rejected as non-fast-forward, and from the
  palette; refused if the remote branch moved since the last fetch.
- Detached HEAD is shown in the sync bar, with push and pull disabled.
- Palette commands for all of the above.
```

- [ ] **Step 3: Full verification**

Run:
```bash
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd frontend && pnpm lint && pnpm test -- --run && cd ..
cd extension && pnpm test -- --run && cd ..
```
Expected: all green.

- [ ] **Step 4: Commit**

```bash
git add docs/USER_GUIDE.md CHANGELOG.md
git commit -m "docs: user guide and changelog for cherry-pick, revert, reset, amend and force push"
```
