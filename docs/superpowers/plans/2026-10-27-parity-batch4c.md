# Feature Parity Batch 4c (Merge and Stash Options) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** "Merge into current branch" offers fast-forward if possible / always merge commit / fast-forward only and squash (remembered per repo), and stashes gain a message, keep-index, include-untracked, per-file stashing, pop, and a stash view with an Untracked group, in both the Tauri app and the VSCode extension.

**Architecture:** `git-core::merge` takes `MergeOptions` (squash writes `SQUASH_MSG` and `cleanup_state`s; `HeadInfo.squash_message` carries it to the commit box). `git-core::stash` takes `StashOptions` (pathspec via `stash_save_ext`, message on a path-limited stash by restamping the new stash commit because git2 0.21 has no message setter), adds `pop_stash`, `StashApplyOutcome` and `stash_files` (tracked files of the stash commit plus the files of its parentless untracked commit, diffed through the existing `getCommitDiff`). The `config` crate remembers merge options per repo. Worker commands, Tauri commands, sidecar methods and `RepoClient` change in lockstep. The frontend adds `MergeDialog`, `StashDialog`, a `SplitButton` primitive, `StashDiffPane`, Pop in the Stashes panel, and palette entries.

**Tech Stack:** Rust (`git2` 0.21, `thiserror`, `serde`/`toml` in `config`, `serde` in tauri-app/sidecar), React + TypeScript, Vitest + Testing Library, WebdriverIO (tauri-driver).

**Spec:** `docs/superpowers/specs/2026-10-27-parity-batch4c-design.md`

## Global Constraints

- No new Rust or npm dependency, no `unsafe`; `docs/LICENSE_COMPLIANCE.md` is untouched.
- `FastForward::{Allow, Never, Only}`, default `Allow`; wire strings `"Allow" | "Never" | "Only"`. `MergeOptions` default `{ Allow, squash: false }`.
- `merge.ff` mapping: `"only"` → `Only`; `false`/`no`/`off`/`0` (case-insensitive) → `Never`; else/unset → `Allow`.
- `SQUASH_MSG` = `Squashed commit of the following:\n` then per commit `\ncommit <sha>\nAuthor: <name> <email>\n\n` + each message line prefixed with four spaces + `\n`. Commits: revwalk target, hide HEAD, `Sort::TOPOLOGICAL | Sort::TIME`.
- Restamped stash: commit message `On <branch>: <message>\n`, reflog message `On <branch>: <message>`, `<branch>` = text between `WIP on ` and the first `:` of the stash's original message.
- Error copy, exactly:
  - `Can't fast-forward: '<name>' has diverged from the current branch.` (`MergeError::NotFastForward`)
  - `There are no changes to stash.` / `Resolve the current conflicts before applying a stash.` / `Commit or unstage your staged changes before applying a stash.` / `Applying this stash would overwrite local changes to the same files. Commit, stash or discard them first.` / `The stash's staged changes conflict with the current files. Apply it without "Restore staged changes".` / `No stash with id <id>.`
- UI copy, exactly: menu item `Merge into current branch…`; dialog aria-label `Merge <branch>`, heading `Merge <branch> into <current>`, legend `Fast-forward`, radios `Fast-forward if possible` / `Always create a merge commit` / `Fast-forward only`, checkbox `Squash`, hint `Stage the combined changes; you write the commit.`, buttons `Merge` / `Cancel`; chevron aria-label `More stash options`; menu items `Stash all` / `Stash…` / `Stash file…`; stash dialog aria-label `Stash changes`, input aria-label `Stash message` placeholder `Optional`, checkboxes `Keep staged changes` / `Include untracked files`, scope line `Only: <path>`, buttons `Stash` / `Cancel`; stash view `section` aria-label `Stash details`, buttons `Apply` / `Pop` / `Drop…`, checkbox `Restore staged changes`, groups `Changes (N)` / `Untracked (M)`, empty text `This stash has no file changes.`; disabled titles `Unstage or commit staged changes first.` / `Resolve conflicts first.`.
- Toasts, exactly: `Already up to date`, `Fast-forwarded to <branch>`, `Squashed <branch>. Review and commit.`, `Stash applied`, `Stash popped`, `Stash applied with conflicts; the stash was kept. Resolve the files, then drop it.` (pop), `Stash applied with conflicts. Resolve the files.` (apply), existing `Changes stashed`.
- Palette ids/labels: `merge-branch:<name>` `Merge <name> into current branch…`; `save-stash` `Stash all changes`; `stash-with-options` `Stash…`; `stash-current-file` `Stash <file>…`; `show-stash:<i>` `Show stash: <msg>`; `pop-stash:<i>` `Pop stash: <msg>`; `apply-stash:<i>` unchanged; `drop-stash:<i>` unchanged label, now selects the stash and opens the Stashes section.
- Wire (Tauri and sidecar, camelCase params): `start_merge { repoPath, branchName, options }`; `get_merge_options { repoPath }` → `{ fastForward, squash }`; `set_merge_options { repoPath, options }` → `null`; `save_stash { repoPath, options: { message, keepIndex, includeUntracked, paths } }` → `null`; `apply_stash` / `pop_stash { repoPath, index, restoreIndex }` → `{ kind: "Applied" } | { kind: "Conflicted", files }`; `get_stash_files { repoPath, commitId }` → `{ tracked, untracked, untrackedCommitId }`; `get_head_info` gains `squashMessage`; `MergeOutcome` gains `{ kind: "Squashed" }`.
- Undo labels: `Merge <name>`, `Squash merge <name>`, `<name>` without a `refs/heads/` or `refs/remotes/` prefix. Stash commands stay unrecorded.
- Tests: `git-core`/`repo-service`/`config` use real temp dirs; frontend mocks `RepoClient` only.
- Every push touching `crates/`, `frontend/src/` or `e2e/` needs a `CHANGELOG.md` update (pre-push hook); the changelog lands in Task 14, so push only after Task 14.

## Review Focus

1. **"Pull has diverged → Merge"** passes `refs/remotes/origin/<b>`; today it fails with "reference not found". Must merge. Pinned in Task 1 (`start_merge_accepts_a_full_remote_tracking_ref`) and Task 8 (`pull merge passes the full upstream ref with Allow`).
2. **Squash then Undo.** The commit box must not stay pre-filled with a stale squash message after undo, reset or a commit. Pinned in Task 2 (`commit_after_squash_has_one_parent_and_clears_squash_msg`, `reset_and_undo_restore_clear_squash_msg`) and Task 5 (`squash_merge_round_trips_and_is_undoable_and_labelled`).
3. **Pop with a conflict must never lose the stash** (libgit2's own `stash_pop` would drop it). Pinned in Task 4 (`pop_keeps_the_stash_and_reports_conflicts`).
4. **Applying with something staged** (common: user staged half their work) must say what to do, not "N uncommitted changes exist in the index". Pinned in Task 4 (`apply_with_staged_changes_reports_staged_changes`) and Task 11 (Apply/Pop disabled with the title).
5. **Message on a file stash** must not corrupt the stash list (older entries, indices, `stash@{1}` content). Pinned in Task 3 (`path_limited_stash_with_message_keeps_older_entries`).

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/git-core/src/merge.rs` | `FastForward`, `MergeOptions`, `MergeOutcome::Squashed`, `NotFastForward`, `start_merge(.., opts)`, `merge_defaults`, `squash_message`, `clear_squash_message` |
| `crates/git-core/src/repo.rs` | `HeadInfo.squash_message` |
| `crates/git-core/src/commit.rs`, `reset.rs`, `undo.rs` | call `clear_squash_message` |
| `crates/git-core/src/stash.rs` | `StashOptions`, `StashApplyOutcome`, `StashFiles`, new `StashError` variants, `save_stash`, `apply_stash`, `pop_stash`, `stash_files`, restamp |
| `crates/git-core/tests/merge.rs`, `tests/stash.rs`, `tests/head_info.rs`, `tests/undo.rs`, `tests/reset.rs` | real-repo tests |
| `crates/config/src/lib.rs`, `crates/config/tests/merge_preferences.rs` (new) | per-repo merge preference |
| `crates/repo-service/src/worker/{mod,merge,stash,undo}.rs` | commands, handlers, handle methods, undo label |
| `crates/tauri-app/src/commands/{mod,merge,stash}.rs`, `main.rs` | commands + DTOs + DTO tests |
| `crates/vscode-sidecar/src/dispatch.rs`, `tests/protocol_roundtrip.rs` | methods + DTOs + round trips |
| `frontend/src/ipc/RepoClient.ts`, `tauriRepoClient.ts`, `vscodeRepoClient.ts` (+ tests, fixtures) | types and methods |
| `frontend/src/state/useMergeRebaseActions.ts`, `useStashActions.ts`, `useAppState.ts` (+ test) | dialogs' open state, options, outcomes |
| `frontend/src/components/MergeDialog.tsx` (+ test, new) | merge options dialog |
| `frontend/src/components/StashDialog.tsx` (+ test, new) | stash options dialog |
| `frontend/src/components/primitives/SplitButton.tsx` (+ test, new unless 4b added one) | main button + chevron menu |
| `frontend/src/components/DiffPane.tsx` (+ test) | split button, "Stash file…", `StashDiffPane`, CommitBox seed |
| `frontend/src/components/CommitBox.tsx` (+ test) | `showAbort` |
| `frontend/src/components/FileActionsMenu.tsx` (+ test) | `stash` action |
| `frontend/src/components/StashPanel.tsx` (+ test), `BranchTree.tsx` (+ test) | Pop + checkbox; menu label |
| `frontend/src/App.tsx`, `App.test.tsx`, `lib/commands.ts` (+ test) | wiring, toasts, palette |
| `e2e/specs/stash-options.spec.ts` (new), `e2e/specs/merge.spec.ts` | E2E |
| `CHANGELOG.md`, `docs/USER_GUIDE.md`, `docs/ARCHITECTURE.md` | docs |

---

### Pre-flight: rebase on main and re-check named code

Batch 4b (refs/remotes) merges to `main` before this batch is implemented.

- [ ] **Step 1: Rebase**

```bash
git fetch origin && git rebase origin/main
```

- [ ] **Step 2: Re-check the names this plan relies on**

Confirm each still exists with the shape below; where 4b changed it, adapt the affected task before starting and note the change in that task's commit message.

- `git_core::merge::start_merge(repo: &Repository, branch_name: &str)`, `MergeOutcome`, `pub(crate) fn conflict_path`, `abort_merge`, `merge_message`.
- `git_core::stash::{save_stash(&mut Repository), apply_stash(&mut Repository, usize), drop_stash, list_stashes}`.
- `git_core::repo::HeadInfo { head_id, detached_at, head_pushed, operation }`, `git_core::diff::{commit_files, commit_diff}`, `git_core::reset::reset_to`, `git_core::undo::restore`, `git_core::commit::commit`.
- `repo-service`: `Command::{StartMerge, SaveStash, ApplyStash, DropStash}`, `undo::plan_for`/`hold_reply`, `history::round_trip` helper.
- `config::ConfigFile` field order (4b may have added a Fetch "Prune" setting).
- Frontend: `BranchTree`'s "Merge into current branch" item, `DiffPane` commit-dock "Stash" button, `FileActionsMenu.fileMenuItems`, `StashPanel` props, `lib/commands.ts` stash entries and `commandGroup`, `useAppState`'s `AppState`/`headInfo` initial value.
- `frontend/src/components/primitives/`: if 4b added a split-button primitive (for the Fetch split button), Task 10 reuses it and skips creating `SplitButton.tsx`.

Run: `cargo test --workspace && (cd frontend && pnpm test -- --run)`
Expected: PASS on the rebased branch before any change.

---

### Task 1: `git-core::merge` options (no-ff, ff-only, full-ref target, `merge.ff` defaults)

**Files:**
- Modify: `crates/git-core/src/merge.rs`
- Test: `crates/git-core/tests/merge.rs` (update every `start_merge(&repo, "feature")` call to pass `MergeOptions::default()`; add the tests below)

**Interfaces:**
- Produces (used by Tasks 2, 5, 6): `FastForward`, `MergeOptions`, `MergeError::NotFastForward(String)`, `start_merge(repo, target: &str, opts: MergeOptions)`, `merge_defaults(repo) -> MergeOptions` as in the spec's "Backend API".

- [ ] **Step 1: Write the failing tests**

Add helpers next to `make_conflicted_repo`, built like it (`create_branch` + `commit_all` + `switch_branch`):
- `fn diverged_repo() -> (TempDir, Repository, String)`: base commit; `feature` adds `feature.txt` (`"new\n"`) in a commit with message `feature commit`; main adds `main.txt` in `main commit`; returns the main branch name, checked out.
- `fn fast_forwardable_repo() -> (TempDir, Repository, String, git2::Oid)`: the setup of `start_merge_fast_forwards_when_current_branch_has_not_diverged`, returning the main branch name and the feature tip.
- Split `make_conflicted_repo` into `make_conflicting_branches()` (everything but the merge) and `make_conflicted_repo()` = that plus `start_merge(&repo, "feature", MergeOptions::default())`, so Task 2 can squash the same branches.

```rust
use git_core::merge::{FastForward, MergeOptions};

fn opts(fast_forward: FastForward) -> MergeOptions {
    MergeOptions { fast_forward, squash: false }
}

#[test]
fn never_creates_a_merge_commit_even_when_fast_forward_is_possible() {
    let (_dir, mut repo, _main, feature_tip) = fast_forwardable_repo();
    let head_before = repo.head().unwrap().peel_to_commit().unwrap().id();

    let outcome = git_core::merge::start_merge(&repo, "feature", opts(FastForward::Never)).unwrap();

    assert_eq!(outcome, MergeOutcome::Merged);
    assert_eq!(repo.state(), git2::RepositoryState::Merge);
    assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().id(), head_before);
    git_core::commit::commit(&mut repo, "Merge feature").unwrap();
    let merge = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(merge.parent_ids().collect::<Vec<_>>(), vec![head_before, feature_tip]);
}

#[test]
fn only_fast_forwards_when_possible() {
    let (_dir, repo, _main, feature_tip) = fast_forwardable_repo();
    let outcome = git_core::merge::start_merge(&repo, "feature", opts(FastForward::Only)).unwrap();
    assert_eq!(outcome, MergeOutcome::FastForwarded);
    assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().id(), feature_tip);
}

#[test]
fn only_refuses_a_diverged_branch_and_changes_nothing() {
    let (dir, repo, _main) = diverged_repo();
    let head_before = repo.head().unwrap().peel_to_commit().unwrap().id();

    let error = git_core::merge::start_merge(&repo, "feature", opts(FastForward::Only)).unwrap_err();

    assert_eq!(error.to_string(), "Can't fast-forward: 'feature' has diverged from the current branch.");
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
    assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().id(), head_before);
    assert!(git_core::status::status(&repo).unwrap().is_empty());
    assert!(!dir.path().join("feature.txt").exists());
}

#[test]
fn start_merge_accepts_a_full_remote_tracking_ref() {
    let (_dir, repo, _main) = diverged_repo();
    let feature_tip = repo.find_reference("refs/heads/feature").unwrap().target().unwrap();
    repo.reference("refs/remotes/origin/feature", feature_tip, true, "test").unwrap();

    let outcome = git_core::merge::start_merge(
        &repo, "refs/remotes/origin/feature", MergeOptions::default()).unwrap();

    assert_eq!(outcome, MergeOutcome::Merged);
}

#[test]
fn merge_defaults_follow_merge_ff() {
    let (_dir, repo) = init_repo();
    assert_eq!(git_core::merge::merge_defaults(&repo), MergeOptions::default());
    repo.config().unwrap().set_str("merge.ff", "false").unwrap();
    assert_eq!(git_core::merge::merge_defaults(&repo).fast_forward, FastForward::Never);
    repo.config().unwrap().set_str("merge.ff", "only").unwrap();
    assert_eq!(git_core::merge::merge_defaults(&repo).fast_forward, FastForward::Only);
    repo.config().unwrap().set_str("merge.ff", "true").unwrap();
    assert_eq!(git_core::merge::merge_defaults(&repo).fast_forward, FastForward::Allow);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test merge`
Expected: FAIL to compile (`FastForward`, `MergeOptions`, three-argument `start_merge` missing).

- [ ] **Step 3: Implement**

- Types and the new error variant exactly as in the spec. `MergeOptions`/`FastForward` derive `Debug, Clone, Copy, PartialEq, Eq, Default`.
- `start_merge(repo, target, opts)`: `let refname = if target.starts_with("refs/") { target.to_string() } else { format!("refs/heads/{target}") };`. Keep the `AlreadyMerging` guard and the up-to-date check first. Fast-forward branch runs only when `analysis.is_fast_forward() && opts.fast_forward != FastForward::Never`. When it is not a fast-forward and `opts.fast_forward == FastForward::Only`, return `NotFastForward(display_name)` where `display_name` strips `refs/heads/` / `refs/remotes/` from `target`. This task ignores `opts.squash`; Task 2 adds it.
- `merge_defaults`: `repo.config()` then `get_string("merge.ff")`; map per Global Constraints; any config error → default.

- [ ] **Step 4: Fix callers so the workspace compiles**

`crates/repo-service/src/worker/merge.rs` `start`: pass `git_core::merge::MergeOptions::default()` for now (Task 5 threads the real options). Run `cargo build --workspace`.

- [ ] **Step 5: Run tests**

Run: `cargo test -p git-core --test merge && cargo test --workspace`
Expected: PASS.

- [ ] **Step 6: Lint and commit**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates/git-core crates/repo-service
git commit -m "feat(git-core): merge options for no-ff, ff-only and full-ref targets"
```

---

### Task 2: `git-core` squash merge and `SQUASH_MSG` lifecycle

**Files:**
- Modify: `crates/git-core/src/merge.rs`, `crates/git-core/src/repo.rs` (`HeadInfo`), `crates/git-core/src/commit.rs`, `crates/git-core/src/reset.rs`, `crates/git-core/src/undo.rs` (`restore`)
- Test: `crates/git-core/tests/merge.rs`, `crates/git-core/tests/head_info.rs`, `crates/git-core/tests/reset.rs`, `crates/git-core/tests/undo.rs`; fix every `HeadInfo { .. }` literal (`cargo build --workspace --tests` lists them: `repo-service` worker tests, `tauri-app` `head_info_dto_serializes_camel_case_with_operation_name`)

**Interfaces:**
- Consumes: Task 1's `start_merge`/`MergeOptions`.
- Produces: `MergeOutcome::Squashed`, `squash_message(repo) -> Option<String>`, `pub(crate) clear_squash_message(repo) -> std::io::Result<()>`, `HeadInfo.squash_message: Option<String>`.

- [ ] **Step 1: Write the failing tests**

`tests/merge.rs`:

```rust
fn squash() -> MergeOptions { MergeOptions { fast_forward: FastForward::Allow, squash: true } }

#[test]
fn squash_stages_the_changes_writes_squash_msg_and_leaves_no_merge_state() {
    let (dir, repo, _main) = diverged_repo();
    let head_before = repo.head().unwrap().peel_to_commit().unwrap().id();
    let feature_tip = repo.find_reference("refs/heads/feature").unwrap().target().unwrap();

    let outcome = git_core::merge::start_merge(&repo, "feature", squash()).unwrap();

    assert_eq!(outcome, MergeOutcome::Squashed);
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
    assert!(!repo.path().join("MERGE_HEAD").exists());
    assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().id(), head_before);
    let staged: Vec<_> = git_core::status::status(&repo).unwrap().into_iter().filter(|e| e.staged).collect();
    assert_eq!(staged.len(), 1);
    assert!(dir.path().join("feature.txt").exists());
    let msg = git_core::merge::squash_message(&repo).unwrap();
    assert!(msg.starts_with("Squashed commit of the following:\n\ncommit "));
    assert!(msg.contains(&format!("commit {feature_tip}\nAuthor: Test User <test@example.com>\n\n    feature commit\n")));
    let info = git_core::repo::head_info(&repo).unwrap();
    assert_eq!(info.operation, None);
    assert_eq!(info.squash_message.as_deref(), Some(msg.as_str()));
}

#[test]
fn squash_does_not_move_head_even_when_fast_forward_is_possible() { /* fast_forwardable_repo; Squashed; HEAD unchanged; feature.txt staged */ }

#[test]
fn squash_with_a_conflict_reports_it_and_leaves_clean_state() {
    // make_conflicting_branches(), merged with squash()
    // assert Conflicted { files: vec!["shared.txt".into()] },
    // repo.state() Clean, index has conflicts, SQUASH_MSG present
}

#[test]
fn commit_after_squash_has_one_parent_and_clears_squash_msg() {
    let (_dir, mut repo, _main) = diverged_repo();
    let head_before = repo.head().unwrap().peel_to_commit().unwrap().id();
    git_core::merge::start_merge(&repo, "feature", squash()).unwrap();

    git_core::commit::commit(&mut repo, "Squash feature").unwrap();

    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.parent_ids().collect::<Vec<_>>(), vec![head_before]);
    assert_eq!(git_core::merge::squash_message(&repo), None);
}

#[test]
fn abort_after_a_conflicted_squash_clears_squash_msg() { /* conflicted squash; abort_merge Ok; squash_message None; no conflicts */ }
```

`tests/head_info.rs`: extend an existing assertion so a fresh repo reports `squash_message: None`.

`tests/reset.rs` and `tests/undo.rs`: `reset_and_undo_restore_clear_squash_msg` — write `<gitdir>/SQUASH_MSG` by hand, `reset_to(&repo, head, ResetMode::Mixed)` → file gone; and in `tests/undo.rs`, capture/restore round trip (copy the smallest existing restore test's setup) with `SQUASH_MSG` present before `restore` → gone after a successful restore.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core`
Expected: FAIL to compile (`Squashed`, `squash_message`, `HeadInfo.squash_message` missing).

- [ ] **Step 3: Implement**

- `start_merge`: after the up-to-date check, `if opts.squash { return squash_merge(repo, &their_reference, &their_annotated, their_commit.id()); }` (before the fast-forward branch). `squash_merge`: `repo.merge(&[annotated], None, None)?`, collect conflict files exactly as the existing tail does (extract that loop into `fn conflicted_files(index: &git2::Index) -> Result<Vec<String>, MergeError>` and reuse it), build the message (Global Constraints) with a revwalk `push(their_id)`, `hide(head_id)`, write it to `repo.path().join("SQUASH_MSG")`, `repo.cleanup_state()?`, return `Conflicted { files }` or `Squashed`.
- `squash_message`: `std::fs::read_to_string(repo.path().join("SQUASH_MSG")).ok()`.
- `clear_squash_message`: remove the file, `Ok(())` on `NotFound`.
- `CommitError` and `ResetError` have no io variant today: add `#[error("{0}")] Io(#[from] std::io::Error)` to both.
- `repo::head_info`: fill `squash_message: crate::merge::squash_message(repo)` in both returns.
- Call `crate::merge::clear_squash_message(repo)?` at the end of `commit::commit`, at the end of `abort_merge` (`MergeError::Io` exists), after `repo.reset` in `reset_to`, and at the end of a successful `undo::restore` (`UndoError::Io` exists).

- [ ] **Step 4: Run tests**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 5: Lint and commit**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates
git commit -m "feat(git-core): squash merge with SQUASH_MSG, cleared on commit, abort, reset and undo"
```

---

### Task 3: `git-core::stash` save options (message, keep index, untracked, paths)

**Files:**
- Modify: `crates/git-core/src/stash.rs`
- Test: `crates/git-core/tests/stash.rs` (existing `save_stash(&mut repo)` calls become `save_stash(&mut repo, &StashOptions::default())`)

**Interfaces:**
- Produces: `StashOptions` (+ `Default`), `StashError::NothingToStash`, `save_stash(repo: &mut Repository, opts: &StashOptions) -> Result<(), StashError>`.

- [ ] **Step 1: Write the failing tests**

```rust
use git_core::stash::{StashError, StashOptions};

fn with(f: impl FnOnce(&mut StashOptions)) -> StashOptions {
    let mut opts = StashOptions::default();
    f(&mut opts);
    opts
}

#[test]
fn stash_with_a_message_lists_it() {
    // tracked change; save_stash(with(|o| o.message = Some("half done".into())))
    // list[0].message starts_with "On " and ends_with ": half done"
}

#[test]
fn blank_message_uses_the_default_wip_message() {
    // message Some("   ") → list[0].message starts_with "WIP on "
}

#[test]
fn keep_index_leaves_staged_changes_staged_and_stashes_them_too() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a.txt", "v1");
    commit_all(&repo, "initial");
    write_file(dir.path(), "a.txt", "staged");
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("a.txt")).unwrap();
    index.write().unwrap();

    git_core::stash::save_stash(&mut repo, &with(|o| o.keep_index = true)).unwrap();

    let status = git_core::status::status(&repo).unwrap();
    assert!(status.iter().any(|e| e.path == "a.txt" && e.staged));
    let stash = repo.find_commit(git2::Oid::from_str(
        &git_core::stash::list_stashes(&mut repo).unwrap()[0].commit_id).unwrap()).unwrap();
    let index_commit = stash.parent(1).unwrap();
    let blob = index_commit.tree().unwrap().get_path(std::path::Path::new("a.txt")).unwrap();
    assert_eq!(repo.find_blob(blob.id()).unwrap().content(), b"staged");
}

#[test]
fn include_untracked_off_leaves_untracked_files() {
    // tracked change + untracked.txt; include_untracked = false → untracked.txt still exists, tracked reverted
}

#[test]
fn path_limited_stash_leaves_other_files() {
    // a.txt and b.txt modified, c.txt untracked; paths = ["a.txt"]
    // a.txt back to committed; b.txt still modified; c.txt still there
}

#[test]
fn path_limited_stash_with_message_keeps_older_entries() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a.txt", "v1");
    write_file(dir.path(), "b.txt", "v1");
    commit_all(&repo, "initial");
    write_file(dir.path(), "b.txt", "older");
    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();
    let older = git_core::stash::list_stashes(&mut repo).unwrap()[0].clone();
    write_file(dir.path(), "a.txt", "mine");
    write_file(dir.path(), "b.txt", "keep");

    git_core::stash::save_stash(&mut repo, &with(|o| {
        o.message = Some("only a".into());
        o.paths = vec!["a.txt".into()];
    })).unwrap();

    let list = git_core::stash::list_stashes(&mut repo).unwrap();
    assert_eq!(list.len(), 2);
    assert!(list[0].message.starts_with("On ") && list[0].message.ends_with(": only a"));
    assert_eq!(list[1].commit_id, older.commit_id);
    assert_eq!(list[1].message, older.message);
    let top = repo.find_commit(git2::Oid::from_str(&list[0].commit_id).unwrap()).unwrap();
    assert_eq!(top.message().unwrap(), format!("{}\n", list[0].message));
    assert_eq!(std::fs::read_to_string(dir.path().join("b.txt")).unwrap(), "keep");
    git_core::stash::apply_stash(&mut repo, 0).unwrap();
    assert_eq!(std::fs::read_to_string(dir.path().join("a.txt")).unwrap(), "mine");
}

#[test]
fn nothing_to_stash_is_a_clear_error() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a.txt", "v1");
    commit_all(&repo, "initial");
    let error = git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap_err();
    assert!(matches!(error, StashError::NothingToStash));
    assert_eq!(error.to_string(), "There are no changes to stash.");
}
```

(Task 4 changes `apply_stash` to three arguments and updates this call with the others.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test stash`
Expected: FAIL to compile.

- [ ] **Step 3: Implement**

- `StashOptions` as in the spec; `Default` = `{ message: None, keep_index: false, include_untracked: true, paths: vec![] }`.
- `save_stash`: trim the message, `None` when empty. Flags from the options (`StashFlags::empty()` plus `KEEP_INDEX`/`INCLUDE_UNTRACKED`). No paths → `stash_save2(&sig, message, Some(flags))`. With paths → `StashSaveOptions::new(sig)`, `.flags(Some(flags))`, `.pathspec(p)` for each, `stash_save_ext(Some(&mut opts))`, then `restamp_top_stash(repo, message)` when a message was given.
- Map `git2::Error` with `code() == ErrorCode::NotFound && class() == ErrorClass::Stash` to `NothingToStash` (save only).
- `fn restamp_top_stash(repo: &Repository, message: &str) -> Result<(), StashError>`, the algorithm the paths/message API gap forces (spec "Decisions → Stash"):

```rust
let old = repo.find_reference("refs/stash")?.peel_to_commit()?;
let original = old.message().unwrap_or_default();
let branch = original.strip_prefix("WIP on ").and_then(|rest| rest.split_once(':')).map(|(b, _)| b).unwrap_or("(no branch)");
let reflog_message = format!("On {branch}: {message}");
let parents: Vec<git2::Commit> = old.parents().collect();
let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
let new_id = repo.commit(None, &old.author(), &old.committer(), &format!("{reflog_message}\n"), &old.tree()?, &parent_refs)?;
repo.reference("refs/stash", new_id, true, &reflog_message)?;   // appends entry 0, original is now 1
let mut reflog = repo.reflog("refs/stash")?;
reflog.remove(1, true)?;
reflog.write()?;
```

- Doc-comment `save_stash` with the git2 0.21 gap (no `StashSaveOptions` message setter).

- [ ] **Step 4: Fix callers so the workspace compiles**

`repo-service` `worker/stash.rs` `save`: pass `&git_core::stash::StashOptions::default()` (Task 5 threads options). `cargo build --workspace`.

- [ ] **Step 5: Run tests**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 6: Lint and commit**

```bash
git add crates
git commit -m "feat(git-core): stash with message, keep-index, untracked toggle and paths"
```

---

### Task 4: `git-core::stash` apply/pop outcomes and `stash_files`

**Files:**
- Modify: `crates/git-core/src/stash.rs`
- Test: `crates/git-core/tests/stash.rs`

**Interfaces:**
- Consumes: `crate::merge::conflict_path` (crate-private, already `pub(crate)`).
- Produces: `StashApplyOutcome`, `StashFiles`, error variants `UnresolvedConflicts`, `StagedChanges`, `WouldOverwrite`, `IndexConflict`, `NotAStash(String)`, and `apply_stash(repo, index, restore_index)`, `pop_stash(repo, index, restore_index)`, `stash_files(repo, commit_id)` as in the spec.

- [ ] **Step 1: Write the failing tests**

```rust
use git_core::stash::{StashApplyOutcome, StashFiles};

#[test]
fn pop_drops_the_stash_on_success() {
    // change; save; pop_stash(&mut repo, 0, true) == Applied; file restored; list empty
}

#[test]
fn pop_keeps_the_stash_and_reports_conflicts() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "file.txt", "base\n");
    commit_all(&repo, "initial");
    write_file(dir.path(), "file.txt", "stashed\n");
    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();
    write_file(dir.path(), "file.txt", "committed\n");
    commit_all(&repo, "conflicting commit");

    let outcome = git_core::stash::pop_stash(&mut repo, 0, false).unwrap();

    assert_eq!(outcome, StashApplyOutcome::Conflicted { files: vec!["file.txt".into()] });
    assert_eq!(git_core::stash::list_stashes(&mut repo).unwrap().len(), 1);
    assert!(repo.index().unwrap().has_conflicts());
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn apply_with_restore_index_restores_staged_state() {
    // stage a.txt change, modify b.txt unstaged; save; apply_stash(.., 0, true) == Applied
    // a.txt staged again, b.txt unstaged; with restore_index false a.txt comes back unstaged
}

#[test]
fn apply_with_staged_changes_reports_staged_changes() {
    // save a stash; then stage an unrelated change; apply_stash → Err(StagedChanges), message per Global Constraints
}

#[test]
fn apply_with_unresolved_conflicts_is_refused() {
    // conflicted pop (as above) leaves conflicts; a second apply_stash → Err(UnresolvedConflicts)
}
```

Tighten `apply_stash_is_blocked_by_a_conflicting_dirty_file`: `matches!(result, Err(StashError::WouldOverwrite))`. Update every existing `apply_stash(&mut repo, 0)` to `apply_stash(&mut repo, 0, false)` and assert `Applied` in `apply_stash_restores_the_stashed_changes`.

```rust
#[test]
fn stash_files_lists_tracked_and_untracked_files() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "v1");
    commit_all(&repo, "initial");
    write_file(dir.path(), "tracked.txt", "v2");
    write_file(dir.path(), "new.txt", "brand new\n");
    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();
    let id = git_core::stash::list_stashes(&mut repo).unwrap()[0].commit_id.clone();

    let files = git_core::stash::stash_files(&mut repo, &id).unwrap();

    assert_eq!(files.tracked, vec!["tracked.txt".to_string()]);
    assert_eq!(files.untracked, vec!["new.txt".to_string()]);
    let untracked_id = files.untracked_commit_id.unwrap();
    let hunks = git_core::diff::commit_diff(&repo, &untracked_id, "new.txt", &Default::default()).unwrap();
    assert_eq!(hunks[0].old_lines, 0);
    assert_eq!(hunks[0].lines[0].content, "brand new\n");
}

#[test]
fn stash_files_without_untracked_has_no_untracked_commit() { /* include_untracked false → untracked empty, id None */ }

#[test]
fn stash_files_rejects_a_commit_that_is_not_a_stash() {
    // HEAD's id → Err(NotAStash(id)), message "No stash with id <id>."
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test stash`
Expected: FAIL to compile.

- [ ] **Step 3: Implement**

- `apply_stash`: if `repo.index()?.has_conflicts()` → `UnresolvedConflicts`. `StashApplyOptions::new()`, `.reinstantiate_index()` when `restore_index`; `repo.stash_apply(index, Some(&mut opts))` with errors mapped: `ErrorCode::Uncommitted` → `StagedChanges`; `ErrorCode::Conflict` with `ErrorClass::Checkout` → `WouldOverwrite`; other `ErrorCode::Conflict` → `IndexConflict`; else `Git`. Then collect conflicted paths from `repo.index()?` (dedup, via `crate::merge::conflict_path`) → `Applied` or `Conflicted { files }`.
- `pop_stash`: `apply_stash`, then `repo.stash_drop(index)?` only on `Applied`. Doc-comment why `git2::Repository::stash_pop` is not used (it drops on conflicts).
- `stash_files`: find the entry via `list_stashes` whose `commit_id == commit_id`, else `NotAStash(commit_id.into())`. `tracked = crate::diff::commit_files(repo, commit_id)`; when the commit has a third parent, `untracked_commit_id = Some(parent(2).id())` and `untracked = commit_files(that id)`. Map `DiffError` into `StashError::Git` (`DiffError::Diff` wraps `git2::Error`).

- [ ] **Step 4: Fix callers**

`repo-service` `worker/stash.rs` `apply`: call `apply_stash(repo, index, false).map(|_| ())` for now. `cargo build --workspace`.

- [ ] **Step 5: Run tests, lint, commit**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates
git commit -m "feat(git-core): stash pop, apply outcomes with conflicts, and stash file lists"
```

---

### Task 5: `config` merge preference and `repo-service` commands

**Files:**
- Modify: `crates/config/src/lib.rs` (types; `merge_preferences: Vec<MergePreference>` as the **last** `ConfigFile` field, `#[serde(default)]`)
- Create: `crates/config/tests/merge_preferences.rs`
- Modify: `crates/repo-service/src/worker/mod.rs` (commands, dispatch), `worker/merge.rs`, `worker/stash.rs`, `worker/undo.rs` (label, tests)

**Interfaces:**
- Consumes: Tasks 1–4.
- Produces (used by Task 6):
  - `config::{MergeFastForward, MergePreference, get_merge_preference(_at), set_merge_preference(_at)}` (spec signatures).
  - `WorkerHandle::start_merge(&self, branch_name: String, options: MergeOptions) -> Result<MergeOutcome, String>`
  - `WorkerHandle::merge_defaults(&self) -> Result<MergeOptions, String>`
  - `WorkerHandle::save_stash(&self, options: StashOptions) -> Result<(), String>`
  - `WorkerHandle::apply_stash(&self, index: usize, restore_index: bool) -> Result<StashApplyOutcome, String>`
  - `WorkerHandle::pop_stash(&self, index: usize, restore_index: bool) -> Result<StashApplyOutcome, String>`
  - `WorkerHandle::get_stash_files(&self, commit_id: String) -> Result<StashFiles, String>`

- [ ] **Step 1: Write the failing config tests**

Mirror `tests/graph_branch_selection.rs`: `get_merge_preference_at_returns_none_for_a_repo_with_no_saved_preference`, `set_merge_preference_at_persists_and_round_trips` (`Never`, `squash: true`), `..._overwrites_a_previous_preference_for_the_same_repo`, `..._keeps_preferences_for_other_repos_separate`, and `merge_preferences_coexist_with_graph_selections` (set both, read both back).

- [ ] **Step 2: Implement config and run**

Run: `cargo test -p config`
Expected: PASS after implementing (same retain-then-push pattern as `set_graph_branch_selection_at`).

- [ ] **Step 3: Write the failing worker tests**

In `mod.rs`'s test module (helpers `init_repo`, `write_file`, `commit_all` exist):

```rust
#[test]
fn squash_merge_round_trips_and_is_undoable_and_labelled() {
    // main + diverged feature (copy the setup of start_merge_and_get_merge_message_round_trip_through_the_worker)
    let outcome = handle.start_merge("feature".into(), MergeOptions { fast_forward: FastForward::Allow, squash: true }).unwrap();
    assert_eq!(outcome, MergeOutcome::Squashed);
    assert!(handle.get_head_info().unwrap().squash_message.is_some());
    assert_eq!(handle.get_undo_state().unwrap().undo.as_deref(), Some("Squash merge feature"));
    handle.undo().unwrap();
    assert!(handle.get_status().unwrap().is_empty());
    assert!(handle.get_head_info().unwrap().squash_message.is_none());
}

#[test]
fn stash_options_pop_and_files_round_trip_through_the_worker() {
    // modify a.txt and b.txt; save_stash(StashOptions { message: Some("only a".into()), paths: vec!["a.txt".into()], ..Default::default() })
    // list_stashes()[0].message ends_with ": only a"
    // get_stash_files(id).tracked == ["a.txt"]
    // pop_stash(0, true) == Applied; list_stashes() empty
}

#[test]
fn merge_defaults_round_trip_through_the_worker() { /* merge.ff=only → Only */ }
```

In `undo.rs` tests: update `assert_held!(StartMerge { branch_name: s() })` to include `options: MergeOptions::default()`; update `SaveStash { reply: tx }` to `SaveStash { options: StashOptions::default(), reply: tx }`; add

```rust
#[test]
fn merge_labels_name_squash_and_strip_ref_prefixes() {
    // StartMerge { branch_name: "feature", options: default } → ("Merge feature", Full)
    // squash: true → "Squash merge feature"
    // branch_name "refs/remotes/origin/main" → "Merge origin/main"
}
```

Update the existing worker tests that call `handle.start_merge("feature".into())` / `save_stash()` / `apply_stash(0)` to the new signatures.

- [ ] **Step 4: Run tests to verify they fail**

Run: `cargo test -p repo-service`
Expected: FAIL to compile.

- [ ] **Step 5: Implement**

- Commands per the spec's "repo-service" list. `ApplyStash`/`PopStash` reply `Sender<Result<StashApplyOutcome, String>>`; `GetStashFiles { commit_id: String, reply: Sender<Result<StashFiles, String>> }`; `GetMergeDefaults { reply: Sender<Result<MergeOptions, String>> }`. Drop the `#[allow(dead_code)]` on `StartMerge` and `WorkerHandle::start_merge` if they are now used by Tauri (they are).
- Handle methods use `history.rs`'s `round_trip` pattern (move `round_trip` to `mod.rs` as `pub(super)` if it is private to `history.rs`).
- `plan_for`: `Command::StartMerge { branch_name, options, .. }` → `fit(format!("{} {}", if options.squash { "Squash merge" } else { "Merge" }, short_ref(branch_name)))`, `short_ref` strips `refs/heads/` then `refs/remotes/`.
- `hold_reply` unchanged apart from the pattern compiling with the new field.

- [ ] **Step 6: Run tests, lint, commit**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates/config crates/repo-service
git commit -m "feat(repo-service): merge and stash options, pop and stash files through the worker"
```

---

### Task 6: Tauri commands and sidecar methods

**Files:**
- Modify: `crates/tauri-app/src/commands/mod.rs` (DTOs + tests after `head_info_dto_serializes_camel_case_with_operation_name`), `commands/merge.rs`, `commands/stash.rs`, `crates/tauri-app/src/main.rs` (`use` list and `generate_handler!`)
- Modify: `crates/vscode-sidecar/src/dispatch.rs` (method table, params, DTOs), `crates/vscode-sidecar/tests/protocol_roundtrip.rs`

**Interfaces:**
- Consumes: Task 5's handle methods and `config` functions.
- Produces: the wire methods in Global Constraints.

- [ ] **Step 1: Write the failing Tauri DTO tests**

```rust
#[test]
fn merge_options_dto_round_trips_camel_case() {
    let dto: MergeOptionsDto = serde_json::from_value(serde_json::json!({"fastForward": "Only", "squash": true})).unwrap();
    assert_eq!(git_core::merge::MergeOptions::from(dto),
        git_core::merge::MergeOptions { fast_forward: git_core::merge::FastForward::Only, squash: true });
    assert_eq!(serde_json::to_value(MergeOptionsDto::from(git_core::merge::MergeOptions::default())).unwrap(),
        serde_json::json!({"fastForward": "Allow", "squash": false}));
}

#[test]
fn merge_outcome_dto_has_squashed() {
    assert_eq!(serde_json::to_value(MergeOutcomeDto::from(git_core::merge::MergeOutcome::Squashed)).unwrap(),
        serde_json::json!({"kind": "Squashed"}));
}

#[test]
fn stash_dtos_serialize_camel_case() {
    let opts: StashOptionsDto = serde_json::from_value(serde_json::json!(
        {"message": null, "keepIndex": true, "includeUntracked": false, "paths": ["a.txt"]})).unwrap();
    assert_eq!(git_core::stash::StashOptions::from(opts), git_core::stash::StashOptions {
        message: None, keep_index: true, include_untracked: false, paths: vec!["a.txt".into()] });
    assert_eq!(serde_json::to_value(StashApplyOutcomeDto::from(
        git_core::stash::StashApplyOutcome::Conflicted { files: vec!["f".into()] })).unwrap(),
        serde_json::json!({"kind": "Conflicted", "files": ["f"]}));
    assert_eq!(serde_json::to_value(StashFilesDto::from(git_core::stash::StashFiles {
        tracked: vec!["t".into()], untracked: vec![], untracked_commit_id: None })).unwrap(),
        serde_json::json!({"tracked": ["t"], "untracked": [], "untrackedCommitId": null}));
}
```

Extend `head_info_dto_serializes_camel_case_with_operation_name`'s expected JSON with `"squashMessage": null`, and add one case with `Some("Squashed commit…")`.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p tauri-app`
Expected: FAIL to compile.

- [ ] **Step 3: Implement the Tauri side**

- DTOs: `MergeOptionsDto { fast_forward: FastForwardDto, squash: bool }` (`Serialize, Deserialize, Clone, Copy`, camelCase; `FastForwardDto` unit enum `Allow | Never | Only`), `From` both ways with git-core and with `config::MergeFastForward`; `MergeOutcomeDto::Squashed`; `StashOptionsDto` (Deserialize); `StashApplyOutcomeDto` (kind-tagged, camelCase fields); `StashFilesDto`; `HeadInfoDto.squash_message`.
- `commands/merge.rs`: `start_merge(repo_path, branch_name, options: MergeOptionsDto, state)`; `get_merge_options(repo_path, state) -> Result<MergeOptionsDto, String>` = `config::get_merge_preference(Path::new(&repo_path))` mapped to DTO, else `worker_handle(..)?.merge_defaults()`; `set_merge_options(repo_path, options) -> Result<(), String>` = `config::set_merge_preference`. All `async fn`.
- `commands/stash.rs`: `save_stash(repo_path, options: StashOptionsDto, state)`, `apply_stash(repo_path, index, restore_index, state) -> Result<StashApplyOutcomeDto, String>`, `pop_stash(...)`, `get_stash_files(repo_path, commit_id, state) -> Result<StashFilesDto, String>`.
- Register `get_merge_options`, `set_merge_options`, `pop_stash`, `get_stash_files` in `main.rs`.

- [ ] **Step 4: Write the failing sidecar round trips**

```rust
#[test]
fn merge_options_round_trip_through_the_sidecar() {
    // ConfigDirGuard + repo with diverged feature (as the existing merge round trip sets up)
    // get_merge_options → {"fastForward":"Allow","squash":false}
    // set_merge_options {"fastForward":"Never","squash":false} → null; get_merge_options → Never
    // start_merge {"branchName":"feature","options":{"fastForward":"Allow","squash":true}} → {"kind":"Squashed"}
    // get_head_info → squashMessage starts with "Squashed commit of the following:"
}

#[test]
fn stash_options_files_and_pop_round_trip_through_the_sidecar() {
    // modify a.txt and b.txt; save_stash {"options":{"message":"only a","keepIndex":false,"includeUntracked":true,"paths":["a.txt"]}}
    // list_stashes[0].message ends with ": only a"; get_stash_files {"commitId": id} → tracked ["a.txt"], untrackedCommitId null
    // pop_stash {"index":0,"restoreIndex":true} → {"kind":"Applied"}; list_stashes empty
}
```

Update any existing sidecar test that calls `save_stash`/`apply_stash`/`start_merge` with the old params.

- [ ] **Step 5: Implement the sidecar methods**

Method table entries `get_merge_options`, `set_merge_options`, `pop_stash`, `get_stash_files`; updated params structs (`StartMergeParams.options`, `SaveStashParams { repo_path, options }`, `StashApplyParams { repo_path, index, restore_index }`, `StashFilesParams { repo_path, commit_id }`, `SetMergeOptionsParams { repo_path, options }`); its own DTO copies matching Tauri's; `HeadInfoDto.squash_message`.

- [ ] **Step 6: Run tests, lint, commit**

Run: `cargo test -p tauri-app && cargo test -p vscode-sidecar && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates/tauri-app crates/vscode-sidecar
git commit -m "feat: expose merge and stash options over Tauri and the VSCode sidecar"
```

---

### Task 7: `RepoClient` types and both clients

**Files:**
- Modify: `frontend/src/ipc/RepoClient.ts`, `tauriRepoClient.ts`, `vscodeRepoClient.ts`
- Test: `frontend/src/ipc/tauriRepoClient.test.ts`, `vscodeRepoClient.test.ts`
- Fixtures: every full `RepoClient` literal and every `HeadInfo` literal (`pnpm exec tsc --noEmit -p .` lists them; `useAppState.ts`'s initial `headInfo`, `DiffPane.tsx`'s `NO_HEAD_INFO`)

**Interfaces:**
- Produces (Tasks 8–13): the TypeScript block in the spec's "RepoClient" section, verbatim.

- [ ] **Step 1: Write the failing client tests**

`tauriRepoClient.test.ts`:

```ts
it("invokes the merge and stash option commands", async () => {
  const options = { fastForward: "Never", squash: false } as const;
  vi.mocked(invoke).mockResolvedValueOnce({ kind: "Merged" });
  await expect(tauriRepoClient.mergeBranch("/repo", "feature", options)).resolves.toEqual({ kind: "Merged" });
  expect(invoke).toHaveBeenLastCalledWith("start_merge", { repoPath: "/repo", branchName: "feature", options });

  vi.mocked(invoke).mockResolvedValueOnce(options);
  await tauriRepoClient.getMergeOptions("/repo");
  expect(invoke).toHaveBeenLastCalledWith("get_merge_options", { repoPath: "/repo" });
  await tauriRepoClient.setMergeOptions("/repo", options);
  expect(invoke).toHaveBeenLastCalledWith("set_merge_options", { repoPath: "/repo", options });

  const stash = { message: "m", keepIndex: false, includeUntracked: true, paths: ["a.txt"] };
  await tauriRepoClient.saveStash("/repo", stash);
  expect(invoke).toHaveBeenLastCalledWith("save_stash", { repoPath: "/repo", options: stash });
  await tauriRepoClient.applyStash("/repo", 1, true);
  expect(invoke).toHaveBeenLastCalledWith("apply_stash", { repoPath: "/repo", index: 1, restoreIndex: true });
  await tauriRepoClient.popStash("/repo", 0, false);
  expect(invoke).toHaveBeenLastCalledWith("pop_stash", { repoPath: "/repo", index: 0, restoreIndex: false });
  await tauriRepoClient.getStashFiles("/repo", "abc");
  expect(invoke).toHaveBeenLastCalledWith("get_stash_files", { repoPath: "/repo", commitId: "abc" });
});
```

`vscodeRepoClient.test.ts`: the same calls with `postMessage` assertions for method names and params, in the style of "wires searchCommits and cancelSearch".

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/ipc`
Expected: FAIL.

- [ ] **Step 3: Implement**

Types and methods per the spec; all through `loggedInvoke` (Tauri) / `call` (VSCode). Doc comments: `mergeBranch` accepts a local branch name or a full ref; `getMergeOptions` returns the remembered choice, else `merge.ff`; `applyStash`/`popStash` resolve `Conflicted` instead of rejecting when the stash applied with conflicts (pop keeps the stash then).

- [ ] **Step 4: Fix fixtures**

Run `cd frontend && pnpm exec tsc --noEmit -p .`. Add to full `RepoClient` literals: `getMergeOptions: async () => ({ fastForward: "Allow", squash: false })`, `setMergeOptions: async () => {}`, `popStash: async () => ({ kind: "Applied" })`, `getStashFiles: async () => ({ tracked: [], untracked: [], untrackedCommitId: null })`, and `applyStash: async () => ({ kind: "Applied" })` where it returned void. Add `squashMessage: null` to every `HeadInfo` literal. Repeat until clean.

- [ ] **Step 5: Run tests, lint, commit**

Run: `cd frontend && pnpm test -- --run && pnpm lint`

```bash
git add frontend/src
git commit -m "feat(frontend): RepoClient merge and stash options in both clients"
```

---

### Task 8: State: merge and stash actions, dialogs, outcomes

**Files:**
- Modify: `frontend/src/state/useMergeRebaseActions.ts`, `useStashActions.ts`, `useAppState.ts`
- Test: `frontend/src/state/useAppState.test.ts`

**Interfaces:**
- Consumes: Task 7.
- Produces (Tasks 9–12), on `UseAppStateResult` and `AppState`:
  - `AppState.mergeDialog: { branchName: string } | null`, `mergeOutcome: { branchName: string; outcome: MergeOutcome } | null`, `stashDialog: { paths: string[] } | null`, `stashRestoreIndex: boolean` (initial `true`), `stashOutcome: { action: "apply" | "pop"; outcome: StashApplyOutcome } | null`.
  - `openMergeDialog(branchName: string): void`, `closeMergeDialog(): void`, `getMergeOptions(): Promise<MergeOptions>`, `mergeBranch(branchName: string, options: MergeOptions): Promise<void>` (persists via `setMergeOptions`, closes the dialog, sets `mergeOutcome`), `mergePull(upstreamRef: string): Promise<void>` (Allow, no squash, no persist).
  - `openStashDialog(paths: string[]): void`, `closeStashDialog(): void`, `saveStash(options?: StashOptions): Promise<void>` (default = stash all: `{ message: null, keepIndex: false, includeUntracked: true, paths: [] }`; closes the dialog), `applyStash(index: number): Promise<void>`, `popStash(index: number): Promise<void>` (both use `state.stashRestoreIndex`, set `stashOutcome`), `setStashRestoreIndex(on: boolean): void`, `dropStash` unchanged.

- [ ] **Step 1: Write the failing hook tests**

In `useAppState.test.ts`, following its existing `renderHook` + fake client style:

- `merge with options persists them, merges with them and records the outcome`: `openMergeDialog("feature")`, `mergeBranch("feature", { fastForward: "Never", squash: false })` → `setMergeOptions` called with those options before `mergeBranch`, `mergeBranch` called with `("/repo", "feature", options)`, `state.mergeDialog === null`, `state.mergeOutcome` equals `{ branchName: "feature", outcome: { kind: "Merged" } }`.
- `pull merge passes the full upstream ref with Allow`: `mergePull("refs/remotes/origin/main")` → `client.mergeBranch("/repo", "refs/remotes/origin/main", { fastForward: "Allow", squash: false })`, `setMergeOptions` not called.
- `saveStash defaults to stashing everything` and `saveStash passes dialog options and closes the dialog`.
- `pop uses the restore-index setting and resets a popped selected stash`: stashes `[{ index: 0, commitId: "s0", … }]`, `selectRow({ commitId: "s0" })`, `setStashRestoreIndex(false)`, `popStash(0)` → `client.popStash("/repo", 0, false)`, `selectedRow === "uncommitted"`, `stashOutcome` `{ action: "pop", outcome: { kind: "Applied" } }`.
- `a conflicted pop keeps the stash selected`: `popStash` resolves `Conflicted` → `selectedRow` stays `{ commitId: "s0" }`.

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/state/useAppState.test.ts`
Expected: FAIL.

- [ ] **Step 3: Implement**

Follow the `forcePushConfirm` pattern for dialog state (`setState` updaters). `mergeBranch` runs inside `runMutation`; it calls `client.setMergeOptions` first and does not let its failure block the merge (log through `lib/logger` and continue). `popStash` sets `selectedRow: "uncommitted"` only for `Applied` when the selected row was that stash's commit id (copy `dropStash`'s check). App.tsx's `onMergePull` switches to `appState.mergePull` (the BranchTree prop name stays `onMergePull`).

- [ ] **Step 4: Run tests, lint, commit**

Run: `cd frontend && pnpm test -- --run && pnpm lint`

```bash
git add frontend/src/state frontend/src/App.tsx
git commit -m "feat(frontend): merge and stash option state, outcomes and dialogs' open state"
```

---

### Task 9: Merge dialog, BranchTree item, outcome toasts, merge E2E update

**Files:**
- Create: `frontend/src/components/MergeDialog.tsx`, `MergeDialog.module.css`, `MergeDialog.test.tsx`
- Modify: `frontend/src/components/BranchTree.tsx` (+ test), `frontend/src/App.tsx` (+ `App.test.tsx`), `e2e/specs/merge.spec.ts`

**Interfaces:**
- Consumes: Task 8.
- Produces: `MergeDialog({ branchName, currentBranchName, loadOptions, onMerge, onCancel }: { branchName: string; currentBranchName: string; loadOptions: () => Promise<MergeOptions>; onMerge: (options: MergeOptions) => void; onCancel: () => void })`. BranchTree's `onMergeBranch` prop now means "open the dialog".

- [ ] **Step 1: Write the failing component tests**

`MergeDialog.test.tsx`:
- `preselects the loaded options`: `loadOptions` resolves `{ fastForward: "Only", squash: false }` → radio "Fast-forward only" checked, "Merge" enabled after load (disabled before).
- `merges with the chosen options`: pick "Always create a merge commit", click "Merge" → `onMerge({ fastForward: "Never", squash: false })`.
- `squash disables the fast-forward choices`: check "Squash" → all three radios disabled; Merge → `onMerge({ fastForward: <loaded>, squash: true })`.
- `heading names both branches` (`Merge feature into main`), Escape/Cancel → `onCancel`.

`BranchTree.test.tsx`: the item label is `Merge into current branch…` and selecting it calls `onMergeBranch("feature")` (update the existing test's label).

`App.test.tsx`: opening the merge dialog from the branch menu renders `dialog[aria-label="Merge feature"]`; a `Squashed` outcome shows the toast `Squashed feature. Review and commit.`; `UpToDate` → `Already up to date`; `FastForwarded` → `Fast-forwarded to feature`.

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/components/MergeDialog.test.tsx src/components/BranchTree.test.tsx src/App.test.tsx`
Expected: FAIL.

- [ ] **Step 3: Implement**

- `MergeDialog` on `FormDialog`; a `<form>` whose submit calls `onMerge`. Radios share `name="fast-forward"`. All copy from Global Constraints.
- BranchTree: label change only.
- App: `onMergeBranch={appState.openMergeDialog}`; render `MergeDialog` when `active && appState.state.mergeDialog !== null`, with `loadOptions={appState.getMergeOptions}`, `onMerge={(o) => void appState.mergeBranch(appState.state.mergeDialog!.branchName, o)}`, `onCancel={appState.closeMergeDialog}`, `currentBranchName`. Toast effect on `mergeOutcome` next to the `pullOutcome` effect.

- [ ] **Step 4: Update `e2e/specs/merge.spec.ts`**

After each of the three `mergeButton.click()` calls, confirm the dialog:

```ts
const confirmMerge = await $(`dialog[aria-label="Merge e2e-merge-feature"] button=Merge`);
await confirmMerge.waitForExist({ timeout: 10000 });
await browser.execute((el) => (el as HTMLElement).click(), confirmMerge);
```

(Use each test's own branch name in the aria-label; extract a `confirmMergeDialog(branch: string)` helper at the top of the spec.)

- [ ] **Step 5: Run tests, lint, commit**

Run: `cd frontend && pnpm test -- --run && pnpm lint`

```bash
git add frontend/src e2e/specs/merge.spec.ts
git commit -m "feat(frontend): merge options dialog with remembered choice and outcome toasts"
```

---

### Task 10: Stash split button, stash dialog, "Stash file…", squash seed in CommitBox

**Files:**
- Create: `frontend/src/components/primitives/SplitButton.tsx`, `SplitButton.module.css`, `SplitButton.test.tsx` (skip if 4b added an equivalent; then use it)
- Create: `frontend/src/components/StashDialog.tsx`, `StashDialog.module.css`, `StashDialog.test.tsx`
- Modify: `frontend/src/components/FileActionsMenu.tsx` (+ test), `DiffPane.tsx` (+ test), `CommitBox.tsx` (+ test), `App.tsx`

**Interfaces:**
- Consumes: Task 8.
- Produces:
  - `SplitButton({ label, onClick, disabled, title, menuLabel, items }: { label: string; onClick: () => void; disabled?: boolean; title?: string; menuLabel: string; items: ContextMenuItem[] })`.
  - `StashDialog({ paths, onStash, onCancel }: { paths: string[]; onStash: (options: StashOptions) => void; onCancel: () => void })`.
  - `FileAction` gains `{ kind: "stash" }`; `fileMenuItems(entry, onPick)` adds `Stash file…` for every non-conflicted entry (after "Discard changes…" for unstaged rows; the only item for staged rows).
  - `DiffPane` props: `onSaveStash: () => void` (stash all, unchanged name) and new `onOpenStashDialog: (paths: string[]) => void`.
  - `CommitBox` prop `showAbort?: boolean` (default `initialMessage !== undefined`).

- [ ] **Step 1: Write the failing tests**

- `SplitButton.test.tsx`: main click calls `onClick`; chevron (aria-label = `menuLabel`, `aria-haspopup="menu"`) opens a menu with the items; selecting one runs it and closes the menu; `disabled` disables both buttons.
- `StashDialog.test.tsx`: defaults (message empty, "Keep staged changes" unchecked, "Include untracked files" checked); submitting with message `" wip "` calls `onStash({ message: "wip", keepIndex: false, includeUntracked: true, paths: [] })`; an empty message sends `null`; with `paths={["a.txt"]}` shows `Only: a.txt` and passes the paths; Cancel → `onCancel`.
- `FileActionsMenu.test.tsx`: unstaged modified entry → labels `["Discard changes…", "Stash file…"]`; staged entry → `["Stash file…"]`; conflicted → `[]` (or not rendered, matching current `hasFileActions`).
- `DiffPane.test.tsx`: the dock still has a `button` named "Stash" calling `onSaveStash`; "More stash options" → "Stash…" calls `onOpenStashDialog([])`; a staged row's "Actions for <path>" → "Stash file…" calls `onOpenStashDialog(["<path>"])`; with `mergeMessage={null}` and `headInfo.squashMessage = "Squashed commit…"` the commit textarea holds the squash message and no "Abort merge" button renders.
- `CommitBox.test.tsx`: `showAbort={false}` with an `initialMessage` hides the abort button.

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/components`
Expected: FAIL.

- [ ] **Step 3: Implement**

- `SplitButton`: two adjacent buttons in a `Toolbar`-like wrapper; the menu is `ContextMenu` positioned from the chevron's `getBoundingClientRect()` (bottom-left).
- DiffPane: replace the dock's Stash `button` with `SplitButton label="Stash" menuLabel="More stash options"`, same disabled rule (`status.length === 0 || rebaseProgress !== null`), items "Stash all" (`onSaveStash`) and "Stash…" (`onOpenStashDialog([])`). Make the file actions menu render for staged non-conflicted rows too (rename `hasFileActions` semantics: menu shown when `fileMenuItems(entry, …)` is non-empty); route `{ kind: "stash" }` to `onOpenStashDialog([entry.path])`. Pass `initialMessage={mergeMessage ?? headInfo.squashMessage ?? undefined}` and `showAbort={mergeMessage !== null}` to `CommitBox`.
- App: `onSaveStash={announce("Changes stashed", () => appState.saveStash())}`, `onOpenStashDialog={appState.openStashDialog}`; render `StashDialog` when `active && state.stashDialog !== null`, `onStash={(o) => void announce("Changes stashed", appState.saveStash)(o)}`.

- [ ] **Step 4: Run tests, lint, commit**

Run: `cd frontend && pnpm test -- --run && pnpm lint`

```bash
git add frontend/src
git commit -m "feat(frontend): stash split button, stash dialog and per-file stash"
```

---

### Task 11: Stash view and Stashes panel Pop

**Files:**
- Modify: `frontend/src/components/DiffPane.tsx` (new `StashDiffPane` next to `CommitDiffPane`), `DiffPane.test.tsx`, `StashPanel.tsx`, `StashPanel.test.tsx`, `App.tsx`

**Interfaces:**
- Consumes: Tasks 7–8 (`getStashFiles`, `applyStash`, `popStash`, `dropStash`, `stashRestoreIndex`).
- Produces: `DiffPane` props `stashes?: StashEntry[]`, `stashActions?: { onApply: (index: number) => void; onPop: (index: number) => void; onDrop: (index: number) => void; restoreIndex: boolean; onSetRestoreIndex: (on: boolean) => void; disabledReason: string | null }`. `StashPanel` gains `onPopStash`, `restoreIndex`, `onSetRestoreIndex`, `applyDisabledReason: string | null`.

- [ ] **Step 1: Write the failing tests**

`DiffPane.test.tsx` (`describe("stash view")`, client mock with `getStashFiles` resolving `{ tracked: ["a.txt"], untracked: ["new.txt"], untrackedCommitId: "u1" }`, `getCommitDiff` recording calls):
- selecting `{ commitId: "s0" }` with `stashes=[{ index: 0, message: "On main: wip", commitId: "s0" }]` renders `section` "Stash details" with heading `On main: wip`, `stash@{0}`, groups `Changes (1)` and `Untracked (1)`; `getCommitDiff` is called with `("/repo", "s0", "a.txt", …)` and `("/repo", "u1", "new.txt", …)`; no "History" button.
- an untracked-less stash hides the Untracked group; an empty one shows `This stash has no file changes.`
- Apply / Pop call `onApply(0)` / `onPop(0)`; Drop… opens the confirm (`Drop "On main: wip"? This cannot be undone.`) and confirming calls `onDrop(0)`; the checkbox reflects `restoreIndex` and calls `onSetRestoreIndex(false)`.
- `disabledReason: "Unstage or commit staged changes first."` disables Apply and Pop with that title; Drop stays enabled.
- a commit id that is not a stash still renders `CommitDiffPane` (existing tests keep passing).

`StashPanel.test.tsx`: a Pop button per row calls `onPopStash(index)`; the "Restore staged changes" checkbox; `applyDisabledReason` disables Apply and Pop (not Drop) with the title.

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/components/DiffPane.test.tsx src/components/StashPanel.test.tsx`
Expected: FAIL.

- [ ] **Step 3: Implement**

- `DiffPane`: if `selectedRow` is a commit id found in `stashes`, render `StashDiffPane` (keyed by commit id) instead of `CommitDiffPane`. `StashDiffPane` loads `client.getStashFiles` with the same ignore-flag/`InlineError`+Retry pattern as `CommitDiffPane.loadFiles`, reuses `CommitFileSection` without `onOpenFileHistory`, and a local `ConfirmDialog` for Drop with the panel's exact copy.
- App: compute `stashApplyDisabledReason` = `"Resolve conflicts first."` when any status entry is `Conflicted`, else `"Unstage or commit staged changes first."` when any entry is staged, else `repositoryOperationDisabled ? operationDisabledReason : null`. Pass it to both `StashPanel` and `DiffPane.stashActions`. Toast effect on `stashOutcome` with the copy in Global Constraints.

- [ ] **Step 4: Run tests, lint, commit**

Run: `cd frontend && pnpm test -- --run && pnpm lint`

```bash
git add frontend/src
git commit -m "feat(frontend): stash view with untracked files, pop and restore staged changes"
```

---

### Task 12: Palette entries

**Files:**
- Modify: `frontend/src/lib/commands.ts`, `frontend/src/lib/commands.test.ts`, `frontend/src/App.tsx` (pass what the palette needs)

**Interfaces:**
- Consumes: Task 8's actions; `DiffCommands.currentFile`; existing `goToSidebarSection`.

- [ ] **Step 1: Write the failing tests**

In `commands.test.ts`:
- `offers merge-branch per non-current local branch and omits it while an operation runs`: label `Merge feature into current branch…`, running it calls `openMergeDialog("feature")`; absent when `mergeMessage !== null`.
- `stash commands`: `save-stash` label is `Stash all changes` and calls `saveStash()`; `stash-with-options` calls `openStashDialog([])`; `stash-current-file` exists only with a current file, label `Stash src/a.ts…`, calls `openStashDialog(["src/a.ts"])`.
- `per-stash commands`: `show-stash:0` selects `{ commitId }`; `pop-stash:0` calls `popStash(0)`.
- `drop-stash no longer drops directly`: running `drop-stash:0` does not call `dropStash`; it selects the stash and calls `goToSidebarSection("Stashes")` (assert through whatever spy the existing "Delete tag" test uses).
- `commandGroup` returns `Branches` for `merge-branch:x` and `Stash` for `stash-with-options`, `stash-current-file`, `show-stash:0`, `pop-stash:0`.

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/lib/commands.test.ts`
Expected: FAIL.

- [ ] **Step 3: Implement**

Per Global Constraints. `merge-branch:*` sits in the `!repositoryOperationDisabled` branch loop next to `switch-branch:*`. Per-stash entries stay behind `panelVisibility.stash`. `stash-current-file` reads `diffCommands?.currentFile`.

- [ ] **Step 4: Run tests, lint, commit**

Run: `cd frontend && pnpm test -- --run && pnpm lint`

```bash
git add frontend/src
git commit -m "feat(frontend): palette entries for merge and stash options; drop stash asks first"
```

---

### Task 13: Tauri E2E flow: stash one file with a message, view it, pop it

**Files:**
- Create: `e2e/specs/stash-options.spec.ts`

- [ ] **Step 1: Write the spec**

Self-contained: its own file, and it neither relies on nor leaves stash entries or staged changes.

```ts
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expandSidebarSection } from "../support/sidebar";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const FILE = "stash-options-e2e.txt";
const MESSAGE = "e2e stash options";

function git(...args: string[]): string {
  return execFileSync("git", args, { cwd: E2E_REPO_PATH }).toString();
}
const contents = () => fs.readFileSync(path.join(E2E_REPO_PATH, FILE), "utf8");

async function click(selector: string) {
  const el = await $(selector);
  await el.waitForExist({ timeout: 10000 });
  await browser.execute((e) => (e as HTMLElement).click(), el);
}

describe("Browsitory stash options", () => {
  before(() => {
    // libgit2 refuses to apply a stash while anything is staged; start from an unstaged index.
    git("reset", "-q");
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), "committed\n");
    git("add", FILE);
    git("commit", "-m", "e2e: stash options fixture");
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), "stash me\n");
  });

  after(() => {
    // Drop any entry this spec left (newest first, so indices stay valid), then restore the file.
    const entries = git("stash", "list", "--format=%gd %gs").split("\n").filter((l) => l.includes(MESSAGE));
    for (const line of entries.reverse()) git("stash", "drop", line.split(" ")[0]);
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), "committed\n");
  });

  it("stashes one file with a message, shows it, and pops it", async () => {
    await click(`button[aria-label="Actions for ${FILE}"]`);
    await click("button=Stash file…");

    const input = await $(`dialog[aria-label="Stash changes"] input[aria-label="Stash message"]`);
    await input.waitForExist({ timeout: 10000 });
    await browser.execute((el, value) => {
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
      setter?.call(el, value);
      el.dispatchEvent(new Event("input", { bubbles: true }));
    }, input, MESSAGE);
    await click(`dialog[aria-label="Stash changes"] button=Stash`);
    await browser.waitUntil(() => contents() === "committed\n", { timeout: 10000, timeoutMsg: "file was not stashed" });

    await expandSidebarSection("Stashes");
    await click(`li*=${MESSAGE}`);
    const details = await $(`section[aria-label="Stash details"]`);
    await details.waitForExist({ timeout: 10000 });
    await details.$(`*=${FILE}`).waitForExist({ timeout: 10000 });

    await browser.execute((el) => (el as HTMLElement).click(), await details.$("button=Pop"));
    await browser.waitUntil(() => contents() === "stash me\n", { timeout: 10000, timeoutMsg: "pop did not restore the file" });
    await browser.waitUntil(() => !git("stash", "list", "--format=%gs").includes(MESSAGE), {
      timeout: 10000,
      timeoutMsg: "the popped stash is still listed",
    });
  });
});
```

- [ ] **Step 2: Run the Tauri E2E**

Exact sequence from CLAUDE.md "E2E: Tauri" (frontend build with `VITE_E2E_REPO_PATH`, `cargo build --workspace --features tauri-app/custom-protocol,tauri-app/forge-fixture-override`, `cd e2e && pnpm install && pnpm test`), under `xvfb-run` on a headless machine. Iterate with `pnpm test -- --spec ./specs/stash-options.spec.ts`, then run the full suite (`merge.spec.ts` and `stash-management.spec.ts` must still pass).
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add e2e/specs/stash-options.spec.ts
git commit -m "test(e2e): stash one file with a message, view it, pop it"
```

---

### Task 14: Docs

**Files:**
- Modify: `CHANGELOG.md` (under `## [Unreleased]`, after "Commit search" and after whatever 4b added), `docs/USER_GUIDE.md` ("Branches and remotes", "Stashes"), `docs/ARCHITECTURE.md` ("git2 API gotchas")

- [ ] **Step 1: CHANGELOG**

```markdown
### Merge and stash options

- "Merge into current branch…" asks how to merge: fast-forward if possible, always create a
  merge commit, or fast-forward only, plus **Squash** (stages the combined changes and pre-fills
  the commit message). The choice is remembered per repository; the first default follows
  `merge.ff`.
- Stash is a split button: **Stash all** or **Stash…** with a message, "Keep staged changes" and
  "Include untracked files". **Stash file…** in a changed file's menu stashes just that file.
- Click a stash to see its changes, including untracked files, and Apply, **Pop** or Drop it from
  there or from the Stashes panel. "Restore staged changes" brings staged work back staged.
  Conflicts leave the stash in place and open in the conflict pane.
- Fixed: Merge in the "Pull has diverged" dialog failed with "reference not found".
- Fixed: "Drop stash" in the command palette no longer drops without asking.
```

- [ ] **Step 2: USER_GUIDE**

- "Branches and remotes": add a "Merge options" paragraph: the three choices and what each does, Squash (nothing is committed; HEAD does not move; commit message pre-filled; Undo reverts), remembered per repository, first default from `merge.ff`; the pull dialog's Merge always allows fast-forward.
- Rewrite "Stashes": split button (Stash all, Stash…), the dialog fields, Stash file… (one file at a time; file names with `*`, `?` or `[` may match other files too), clicking a stash shows Changes and Untracked, Apply / Pop / Drop…, "Restore staged changes" (on by default), Apply/Pop need nothing staged, a conflicting Pop keeps the stash and the files open in the conflict pane, dropping asks first and cannot be undone, stash operations are not undoable.

- [ ] **Step 3: ARCHITECTURE**

Add to "git2 API gotchas":

```markdown
- `StashSaveOptions` (git2 0.21) has no message setter, so `stash_save_ext` with a pathspec
  always writes "WIP on …". `stash::save_stash` re-commits the new stash with the message and
  swaps the top `refs/stash` reflog entry ("restamp").
- `Repository::stash_pop` drops the stash even when the apply left conflicts; Browsitory applies
  and drops itself. `stash_apply` refuses whenever the index differs from HEAD.
- `cleanup_state` does not remove `SQUASH_MSG`; `merge::clear_squash_message` does, after a
  commit, an abort, a reset or an undo.
```

- [ ] **Step 4: Full verification and commit**

Run:
```bash
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd frontend && pnpm lint && pnpm test -- --run && pnpm build && cd ..
cd extension && pnpm test -- --run && pnpm run compile && cd ..
```
Expected: all PASS.

```bash
git add CHANGELOG.md docs/USER_GUIDE.md docs/ARCHITECTURE.md
git commit -m "docs: merge and stash options in user guide, changelog and architecture"
```
