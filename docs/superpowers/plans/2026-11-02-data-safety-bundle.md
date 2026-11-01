# Data-Safety Bundle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close seven data-safety follow-ups in five parallel PRs:

- PR 1 (`fix/stash-in-the-way`): **#206** mirror stash apply; **#208** ignored entries in the
  way; **#211** nested repositories in the way.
- PR 2 (`fix/pick-leftover-author`): **#226** leftover `CHERRY_PICK_HEAD`.
- PR 3 (`fix/repo-identity`): **#223** one worker and gate per repository, whatever the spelling.
- PR 4 (`fix/worktree-parents`): **#200** worktree add creates missing parents, rolls back.
- PR 5 (`fix/read-only-writes`): **#209** clear refusal for read-only files.

**Architecture:**

- `git-core`: `stash.rs` (`entry_in_the_way`, `refuse_untracked_below_tracked_file`),
  `commit.rs` (`commit_with`), `pick.rs` (`commit_carries_pick`), `worktree.rs`
  (`create_worktree_with`), `worktree_guard.rs` + `beneath_{unix,windows}.rs` (`ReadOnlyFile`),
  `merge.rs`, `stage.rs`.
- `repo-service::worker`: `registry.rs` (new), `mod.rs` (`ensure_worker` removed).
- `tauri-app`: `commands/mod.rs` (`AppState`, `open_repo`, `forget_repo`, `repo_worker`).
- `vscode-sidecar`: `dispatch.rs`, `main.rs`.

**Tech Stack:** Rust (`git2` 0.21 / libgit2 1.9.6, `libc`, `windows-sys` as already used),
tokio via `tauri::async_runtime`.

**Spec:** `docs/superpowers/specs/2026-11-02-data-safety-bundle-design.md`. This plan implements
the **provisional** choice of each decision D1–D7. "Decision hooks" at the end lists what changes
for the other options.

---

## Global Constraints

- **Branches:** each PR from `main`; the five run in parallel; open each only after its gate task
  (SZ, CZ, PZ, WZ, RZ). Rebase on `main` before merging; CHANGELOG conflicts (each PR's section
  at the top of `## [Unreleased]`) are resolved by keeping both sections.
- **No new dependency.**
- **Tests use real temp-dir repos** (`tempfile` + `git2::Repository::init`); git-core
  integration tests start with `mod common;` (hermetic git via `test_support::hermetic_git!()`);
  git CLI twins via `tests/stash.rs`'s `git()` helper (l.1135) or a local copy.
- **Platform gating:** permission tests `#[cfg(unix)]` and skip when `libc::geteuid() == 0`;
  Windows walker tests `#[cfg(windows)]` (CI `windows-latest` only); case tests
  `#[cfg(any(windows, target_os = "macos"))]`.
- **Never** touch `~/.local/share/com.browsitory.app`, the real config dir or `~/.gnupg`; probes
  go under `target/` and are deleted.
- **New user-visible strings** (exact, spec §1–6): `UntrackedBelowTrackedFile`,
  `IgnoredInTheWay`, `RepositoryInTheWay`, `LeftoverPickState`, `WorktreeError::CreateFolder`,
  `MergeError::ReadOnlyFile`, `StageError::ReadOnlyFile`.
- **CHANGELOG:** every push touching `crates/`, `frontend/src/` or `e2e/` needs a `CHANGELOG.md`
  change (pre-push hook). Each PR's first task adds a stub section at the top of
  `## [Unreleased]`; its Z task writes the final text.
- **Commits:** one per task unless noted, `<area>: <what> (#NNN)`, ending with the session's
  `Co-Authored-By`/`Claude-Session` trailers.

## Pre-flight

- [ ] `git status` clean, rebased on `origin/main`; if `main` moved past 34d5576b, re-check the
  line references below.
- [ ] Baseline: `cargo test --workspace`, `cd frontend && pnpm test -- --run`. Record counts.
- [ ] Pre-push hook linked (CLAUDE.md "Git hooks").

Reconciled with the code at 34d5576b on 2026-11-01:

- `crates/git-core/src/stash.rs`: `StashError` l.6–57, `save_stash` l.144 (in-the-way call
  l.155–159), `save_path_limited_stash` l.229, `untracked_in_the_way` l.483–555, `apply_stash`
  l.627 (`refuse_untracked_file_over_folder` call l.651), `refuse_untracked_file_over_folder`
  l.695–748, `pop_stash` l.753. Tests: `folder_replaced_by_file` l.1101, `file_replaced_by_folder`
  l.1126, `git` l.1135, `top_level` l.1144, `assert_untracked_in_the_way` l.1237.
- `crates/git-core/src/commit.rs`: `CommitError` l.~10–46 (`LeftoverMergeState` l.44–45),
  `cleanup_after_commit` l.432, `message_source` l.462, `commit_with` l.533 (merge guard l.552–560,
  `pick_author` l.562–572, cleanup l.645–655), `merge_already_committed` l.673.
- `crates/git-core/src/pick.rs`: `step` l.331 (cleanup l.436), `finish_paused` l.760 (l.845),
  `is_step_committed_by_hand` l.867, `commit_matches_pick` l.906.
- `crates/git-core/tests/pick.rs` l.736; `tests/hooks.rs` l.1302–1349; `tests/common/mod.rs`
  `ReadOnlyGitDir` l.263.
- `crates/repo-service/src/worker/mod.rs`: `Worker::spawn` l.733, `ensure_worker` l.1905, tests
  l.3528, l.6023, l.6595.
- `crates/tauri-app/src/commands/mod.rs`: `AppState` l.1723, `open_repo` l.1734, `close_repo`
  l.1748, `forget_repo` l.1754, `RepoWorker` l.1999, `repo_worker` l.2056; tests l.2192, 2255,
  2271, 2313, 2427, 3791.
- `crates/vscode-sidecar/src/dispatch.rs`: `Repos` l.34, `worker_handle` l.206, `open_repo` l.221,
  `close_repo` l.241; tests l.4227, 4329, 4370. `main.rs` l.33.
- `crates/git-core/src/worktree.rs`: `create_worktree_with` l.98–146;
  `worktree_guard::nearest_existing` worktree_guard.rs l.419.
- `crates/git-core/src/worktree_guard/beneath_windows.rs`: `delete_classic_with` l.260,
  `Purpose` l.648, `open_leaf_at` l.661, `reopen_same` l.710, `write` l.925 (reopen l.957).
  `beneath_unix.rs`: `stat_at` l.87, `write` l.293.
- `crates/git-core/src/merge.rs`: `MergeError` l.~6–25, `resolve_conflict` l.373 (l.384),
  `resolve_add_delete_conflict` l.722 (l.745). `stage.rs`: `StageError` l.~19–60, `ignore_path`
  l.355 (l.383).

## Pre-flight notes

- `main` was still 34d5576b when the parts started. The user accepted all A choices
  (D1-A … D7-A) on 2026-11-01.
- S1 (characterization on `main`, git 2.53 twins): Stash all deletes an ignored file in the way
  of a restored folder, an ignored folder where HEAD has a file, and an untracked nested
  repository where HEAD has a file, with and without untracked files; `git stash push [-u]`
  deletes all three too. Popping the mirror stash (`file_replaced_by_folder`, Stash all) returned
  `Applied`, left `d~Updated upstream` and dropped the stash; `git stash pop` fails on it.
- S3: a path-limited stash of an untracked file below a tracked symlink (previously
  `SymlinkReplacedByDirectory` from libgit2's "null OID") is now refused earlier as
  `UntrackedBelowTrackedFile`.

---

## Part S: stash (PR 1, `fix/stash-in-the-way`)

### Task S1: characterize what the stash deletes and what Pop does (#206, #208, #211)

**Files:** `crates/git-core/tests/stash.rs`; `CHANGELOG.md` (stub `### Stash: things in the way`).

- [ ] Fixtures:
  - `ignored_file_over_folder()`: commit `.gitignore` = `d\n` and `d/f`; `rm -r d`; write file `d`.
  - `ignored_folder_over_file()`: commit `.gitignore` = `build/\n` and file `build`; `rm build`;
    write `build/out.o`.
  - `nested_repo_over_file()`: commit file `d`; `rm d`; `git2::Repository::init(d)` with one
    commit (`d/inner.txt`).
- [ ] Characterization tests (assert what happens on `main`, each with a git CLI twin in a second
  temp repo recording git's result in a comment and an assertion):
  - `stash_all_deletes_an_ignored_file_in_the_way_today` for `include_untracked` false and true;
  - `stash_all_deletes_an_ignored_folder_where_head_has_a_file_today`;
  - `stash_all_deletes_a_nested_repository_where_head_has_a_file_today` (both modes);
  - `popping_a_stash_whose_folder_replaced_a_file_leaves_a_side_file_today`
    (`file_replaced_by_folder()`, `StashOptions::default()`, `pop_stash` → `Applied`, a
    `d~Updated upstream` in `top_level`, stash dropped); twin `git stash push -u` + `git stash pop`.
- [ ] Record results in "Pre-flight notes"; S2/S3 turn these assertions into the target
  behaviour. Commit `stash tests: characterize entries the reset deletes and the mirror pop (#206, #208, #211)`.

### Task S2: refuse to delete ignored entries and nested repositories (#208, #211, D2-A)

**Files:** `stash.rs`; `tests/stash.rs`.

- [ ] **Failing tests** (replace S1's "today" tests):
  - `stash_all_refuses_to_delete_an_ignored_file_in_the_way` (both modes × keep_index): error
    `IgnoredInTheWay { ignored: "d", tracked: "d/f" }`, exact message, no `refs/stash`, status
    unchanged, `d` intact;
  - `…_an_ignored_folder_where_head_has_a_file` → `{ "build", "build" }`, `build/out.o` intact;
  - `…_a_nested_repository_where_head_has_a_file` (both modes) → `RepositoryInTheWay { "d", "d" }`,
    `d/.git/HEAD` and the inner commit intact;
  - `ignored_folders_elsewhere_do_not_block_a_stash` (`target/` ignored, unrelated edit stashes);
  - existing `UntrackedInTheWay` tests stay green (l.1265–1409).
- [ ] **Implement** spec §2 (`entry_in_the_way`, `Kind`, two variants, the call for every whole
  stash in both modes). Doc comment cites checkout.c `checkout_action_with_wd_blocker` /
  `_with_wd_dir`.
- [ ] Commit `stash: refuse to delete ignored files and nested repositories in the way (#208, #211)`.

### Task S3: refuse a stash whose untracked file needs a folder over a tracked file (#206, D1-A)

**Files:** `stash.rs`; `tests/stash.rs`.

- [ ] **Failing tests:**
  - `applying_or_popping_a_stash_whose_folder_replaced_a_tracked_file_is_refused`: Apply and Pop
    (with and without restore_index) → `UntrackedBelowTrackedFile { untracked: "d/x", file: "d" }`,
    exact message; stash list length 1; `top_level == ["d"]`, `d == "f\n"`; status empty.
  - `a_stashed_untracked_folder_below_a_file_committed_later_is_refused`: stash untracked `d/x`
    (HEAD without `d`), then commit a file `d`, apply → same error with `file: "d"`.
  - `the_refused_stash_applies_after_the_deletion_is_committed`: remove `d` from index + worktree,
    commit; `pop_stash` → `Applied`, `d/x == "x\n"`, stash dropped.
- [ ] **Implement** spec §1. Commit `stash: refuse a stash whose untracked file needs a folder where a file is tracked (#206)`.

### Task SZ: docs, changelog, gate (PR 1)

- [ ] `CHANGELOG.md` → `### Stash: things in the way`:
  - `Fixed: Stash all deleted an ignored file or folder that was in the way of a file or folder it put back, with or without Include untracked files; it now says "Stashing would delete the ignored 'd', which is in the way of the tracked 'd/f'. Move 'd' first." and stashes nothing (#208).`
  - `Fixed: Stash all could delete a repository nested where the branch has a file of that name, including its history; it now refuses with "Stashing would delete the repository in 'd', …" (#211).`
  - `Fixed: applying or popping a stash whose untracked files sit in a folder that replaced a tracked file reported success, left a "d~Updated upstream" file, and Pop dropped the stash. It now says "This stash can't be applied here: its untracked file 'd/x' needs a folder where 'd' is a tracked file. Commit the deletion of 'd' first, then apply it; the stash is kept." (#206).`
- [ ] `docs/ARCHITECTURE.md` "git2 API gotchas" l.125–150 (kinds; ignored/nested in both modes;
  apply merges untracked against the index; the mirror refusal replaces "is not detected";
  keep-index resets once to the index commit, HEAD as baseline; S1's git findings).
  `docs/USER_GUIDE.md` "Stashes" l.472.
- [ ] Gate: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, `check-changelog.py`. PR: D1/D2 answers, S1 findings; "Closes #206,
  closes #208, closes #211".

---

## Part C: cherry-pick leftovers (PR 2, `fix/pick-leftover-author`)

### Task C1: a leftover pick state doesn't author the next commit (#226, D3-A)

**Files:** `crates/git-core/src/pick.rs`, `commit.rs`; `tests/pick.rs`, `tests/pick_sequence.rs`;
`CHANGELOG.md` (stub `### Cherry-pick leftovers`).

- [ ] **Failing tests** (`#[cfg(unix)]`, fixture of `a_cleanup_failure_after_a_clean_pick_still_counts_it`):
  - `a_commit_after_a_leftover_pick_state_is_authored_by_the_user`: after the leftover, restore
    permissions, remove the hook, stage `after.txt`, `commit(..)` → HEAD author = "Test User",
    parent = the pick commit, `repo.state() == Clean`, no `MERGE_MSG`;
  - `a_leftover_pick_state_that_cannot_be_cleared_refuses_the_commit`:
    `ReadOnlyGitDir` during the commit → `CommitError::LeftoverPickState`, exact message, HEAD
    unchanged;
  - `a_leftover_after_a_prepare_commit_msg_rewrite_is_still_recognised` (hook appends a line);
  - `a_paused_conflicted_pick_is_still_committed_with_the_picked_author` (regression);
  - `tests/pick_sequence.rs`: leftover after the last step of `cherry_pick_many` → next commit by
    the user.
- [ ] **Implement** spec §3: split `commit_carries_pick` from `commit_matches_pick` (unchanged
  behaviour for its callers), add the guard and `LeftoverPickState`.
- [ ] Commit `commit: clear a leftover cherry-pick state instead of taking its author (#226)`.

### Task CZ: docs, changelog, gate (PR 2)

- [ ] `CHANGELOG.md` → `### Cherry-pick leftovers`:
  - `Fixed: when git's cherry-pick state couldn't be cleared after a cherry-pick was committed, your next commit was authored as the picked commit's author. Browsitory now recognises the leftover, clears it and commits as you; if it can't be cleared, the commit is refused with "This cherry-pick was already committed, but git's cherry-pick state can't be cleared. …" (#226).`
- [ ] `docs/ARCHITECTURE.md` "Git hooks" l.590–600 (pick leftover next to the merge one).
- [ ] Gate: fmt, clippy, `cargo test --workspace`, `check-changelog.py`. PR: D3 answer; "Closes #226".

---

## Part P: repository identity (PR 3, `fix/repo-identity`)

### Task P1: `WorkerRegistry` (#223, D4-A)

**Files:** `crates/repo-service/src/worker/registry.rs` (new), `worker/mod.rs`; `CHANGELOG.md`
(stub `### Repository identity`).

- [ ] **Failing tests** (inline, real repos via the module's `init_repo`):
  - `spellings_of_one_repository_share_one_worker`: `open(p)`, `open(p + "/")`,
    `open(p/"sub/..")`, `open(p/"sub")` → same key, one worker (`registry.len() == 1`);
  - `#[cfg(unix)] a_symlinked_path_shares_the_worker`;
  - `#[cfg(any(windows, target_os = "macos"))] a_case_variant_shares_the_worker`;
  - `a_linked_worktree_gets_its_own_worker`;
  - `closing_one_spelling_keeps_the_worker_for_the_other` / `closing_the_last_spelling_drops_it`
    (`close` returns the key only then);
  - `get_of_a_spelling_never_opened_is_none`;
  - `a_dead_worker_is_respawned_under_its_key` (port of l.6023's test);
  - `opening_a_folder_that_is_no_repository_fails_as_before` (error text unchanged).
- [ ] **Implement** spec §4 (`repo_key`, `WorkerRegistry::{open,get,close,len}`); remove
  `ensure_worker`, port its tests (l.6023, l.6595) to the registry.
- [ ] Commit `workers: one worker per repository, whatever the path's spelling (#223)`.

### Task P2: Tauri and sidecar use the registry (#223)

**Files:** `crates/tauri-app/src/commands/mod.rs`; `crates/vscode-sidecar/src/{dispatch.rs,main.rs}`.

- [ ] **Failing tests:**
  - Tauri `#[cfg(unix)] two_spellings_share_one_gate`: open `p` and `p/`; hold the gate via `p`
    (as l.2313); a `call` via `p/` waits until it is released;
  - Tauri `closing_one_spelling_keeps_the_gate`; existing #181 tests ported to `registry`;
  - sidecar `a_repository_opened_with_a_trailing_slash_answers_another_spelling` and
    `closing_one_spelling_keeps_the_other_open`.
- [ ] **Implement:** `AppState { registry, gates }` (gates keyed by key; registry → gates lock
  order); `open_repo`, `forget_repo`, `repo_worker`; sidecar `Repos = Arc<WorkerRegistry>`.
- [ ] Commit `tauri, sidecar: key workers and gates by repository, not spelling (#223)`.

### Task PZ: docs, changelog, gate (PR 3)

- [ ] `CHANGELOG.md` → `### Repository identity`:
  - `Fixed: a repository opened through two different paths (a trailing slash, a subfolder, a symbolic link, or different letter case on Windows), for example from a worktree's Open button, got two background workers, so actions in the two tabs could run at the same time. They now share one worker, and closing one tab keeps the other working (#223).`
- [ ] `docs/ARCHITECTURE.md` "Threading model" l.162–224 (registry, key, aliases, gates by key,
  cross-process limits); `docs/USER_GUIDE.md` "Multiple repositories" l.37 (one sentence).
- [ ] Gate: full CLAUDE.md order (Rust, frontend build/lint/test, extension compile/test, both E2E
  suites); Windows/macOS CI green for the case test; `check-changelog.py`. PR: D4 answer, macOS
  result; "Closes #223".

---

## Part W: worktree parents (PR 4, `fix/worktree-parents`)

### Task W1: create missing parents, roll back on failure (#200, D5-A)

**Files:** `crates/git-core/src/worktree.rs`; `crates/git-core/tests/worktree.rs`; `CHANGELOG.md`
(stub `### Worktree add`).

- [ ] **Failing tests:**
  - `creates_missing_parent_folders_of_the_destination`: `dir/a/b/wt` (a, b missing) → Ok, listed;
    twin `git worktree add <twin>/a/b/wt -b x` succeeds too;
  - `a_failed_add_removes_the_parent_folders_it_created`: branch = the main worktree's branch
    (libgit2 "already checked out") → error; `dir/a` doesn't exist;
  - `a_file_on_the_way_is_a_clear_error`: `dir/a` is a file → `CreateFolder`, message names `a/b`
    (or `a`), no branch created;
  - `#[cfg(unix)] a_failed_add_leaves_no_half_made_worktree_or_new_branch`: parent `dir/p`
    `chmod 0555` (skip as root), new branch `x` with start point → error; no
    `.git/worktrees/wt`, no branch `x`; after `chmod 0755` the same call succeeds.
- [ ] **Implement** spec §5 (`created` list, `WorktreeError::CreateFolder`, rollback of folders,
  `worktrees/<name>` and the branch this call created).
- [ ] Commit `worktree: create missing parent folders and clean up a failed add (#200)`.

### Task WZ: docs, changelog, gate (PR 4)

- [ ] `CHANGELOG.md` → `### Worktree add`:
  - `Creating a worktree in a folder that doesn't exist yet now creates the missing folders, as git worktree add does (#200).`
  - `Fixed: a worktree add that failed could leave a half-made worktree entry and the new branch behind, so trying again with the same name failed with "git operation failed"; a failed add now cleans up after itself, and a folder it can't create is named in the error (#200).`
- [ ] `docs/USER_GUIDE.md` "Worktrees and submodules" l.547; `docs/ARCHITECTURE.md` l.763.
- [ ] Gate: fmt, clippy, `cargo test --workspace`, `check-changelog.py`. PR: D5 answer; "Closes #200".

---

## Part R: read-only files (PR 5, `fix/read-only-writes`)

### Task R1: the walker reports a read-only file (#209, D6-A)

**Files:** `crates/git-core/src/worktree_guard.rs`, `worktree_guard/beneath_unix.rs`,
`worktree_guard/beneath_windows.rs`; `CHANGELOG.md` (stub `### Read-only files`).

- [ ] **Failing tests:**
  - `beneath_unix` `#[cfg(unix)] writing_a_read_only_file_is_reported_as_read_only` (0444, skip
    as root): `is_read_only(&err)`, content unchanged; `a_file_without_permission_for_others_keeps_the_raw_error` not needed (owner-only rule documented);
  - `beneath_windows` `writing_a_read_only_file_is_reported_as_read_only`
    (`set_readonly(true)`): `is_read_only`, content and attribute unchanged;
    `writing_a_writable_file_still_works` (regression).
- [ ] **Implement** spec §6 walker part (`ReadOnlyFile`, `read_only_error`, `is_read_only`;
  detection in both `write`s; `write_beneath` doc).
- [ ] Run Windows tests on CI (push the branch; `windows-latest`).
- [ ] Commit `walker: report a read-only file instead of access denied (#209)`.

### Task R2: clear refusals in conflict resolution and Add to .gitignore (#209)

**Files:** `merge.rs`, `stage.rs`; `tests/merge.rs`, `tests/discard_ignore.rs`.

- [ ] **Failing tests** (cross-platform via `std::fs::Permissions::set_readonly(true)`; skip as
  root on Unix):
  - `resolving_a_conflict_in_a_read_only_file_is_refused`: `resolve_conflict` →
    `MergeError::ReadOnlyFile("f.txt")`, exact message; file bytes and the three conflict stages
    unchanged;
  - `taking_a_side_of_an_add_delete_conflict_over_a_read_only_file_is_refused`;
  - `adding_to_a_read_only_gitignore_is_refused`: `ignore_path` → `StageError::ReadOnlyFile(".gitignore")`, file unchanged.
- [ ] **Implement:** the two variants and the mappings at merge.rs l.384, l.745, stage.rs l.383.
- [ ] Commit `conflicts, ignore: say a file is read-only instead of access denied (#209)`.

### Task RZ: docs, changelog, gate (PR 5)

- [ ] `CHANGELOG.md` → `### Read-only files`:
  - `Fixed: resolving a conflict or adding a pattern to .gitignore failed with "Access is denied" (Windows) or "Permission denied" when the file was read-only; it now says "'f.txt' is read-only. Make it writable, then try again." and changes nothing (#209).`
- [ ] `docs/ARCHITECTURE.md` "Worktree writes and symlinks" l.685–700 (writes refuse read-only
  files; deletes still clear the attribute, #194); `docs/USER_GUIDE.md` "Resolving conflicts"
  l.108, "Discarding and ignoring files" l.139.
- [ ] Gate: fmt, clippy, `cargo test --workspace`; Windows CI green (record the job id in the PR);
  `check-changelog.py`. PR: D6 answer; "Closes #209".

---

## Decision hooks

| Decision | If the user picks… | Change |
|---|---|---|
| D1-B | own ordered apply | S3 becomes a separate PR: `apply_stash` for stashes with an untracked tree runs libgit2's merge of base/index/stash trees + `checkout_index` (baseline = index), then writes untracked files through `worktree_guard::replace_beneath` refusing existing paths; `FileReplacedFolder` and `UntrackedBelowTrackedFile` tests flip to success. |
| D1-C | refuse creating | S3 also adds a save-time check in `save_stash` (untracked file below a tracked path the worktree deletes) with its own message. |
| D2-B | nested repos only | S2 drops `Kind::Ignored`; S1's ignored tests stay as documentation; USER_GUIDE notes ignored files are deleted, as git does. |
| D2-C | document only | Drop S2; S1 tests stay as characterization. |
| D3-B | message must match | C1 calls `commit_matches_pick` with `[picked message, MERGE_MSG]`; the rewrite test expects the picked author (documented limitation). |
| D3-C | refuse until Abort | C1: on a detected leftover return `CommitError::OperationInProgress`-like "This cherry-pick was already committed; click Abort to clear it." |
| D4-B | frontend identity | P3 (new task): `open_repo` returns the key's display path (`\\?\` stripped); `useOpenRepos` dedups tabs by it and persists it; config migration for `open_repos`/`recent_repos`/graph selections; Vitest + both clients. |
| D4-C | lexical only | P1's `repo_key` = `Path::components()` collect + trailing-separator trim (no discover/canonicalize); drop the symlink/case/subfolder tests. |
| D5-B | refuse | W1: before the branch, `nearest_existing(path) != path.parent()` → `WorktreeError::ParentMissing(parent)` ("The folder '…' doesn't exist."); keep the rollback. |
| D5-C | via walker | W1 adds `worktree_guard::create_dirs_beneath(root, rel)` (both `beneath_*` `walk(.., OnMissing::Create)`), touching PR 5's files: serialize PR 4 after PR 5. |
| D6-B | clear and write | R1: Windows clears `FILE_ATTRIBUTE_READONLY` through a `FILE_WRITE_ATTRIBUTES` leaf handle then reopens; Unix opens `O_RDONLY|O_NOFOLLOW`, `fchmod(+S_IWUSR)`, reopens and checks dev/ino; R2 tests expect the write. |
| D6-C | clear, write, restore | D6-B plus restoring the original attribute/mode after the write (failure logged). |
| D7-B | four PRs | Parts C and W on `fix/small-safety`, two CHANGELOG sections. |
| D7-C | one PR | All parts on `fix/data-safety-bundle`, sections as above. |
