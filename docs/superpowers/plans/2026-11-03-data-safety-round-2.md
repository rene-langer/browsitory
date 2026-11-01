# Data-Safety Round 2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close four data-safety follow-ups in four parallel PRs:

- PR 1 (`fix/stash-index-removed`): **#247** Stash all keeps edits to files removed from the index.
- PR 2 (`fix/keep-sequencer`): **#241** committing a terminal sequence's step keeps its todo.
- PR 3 (`fix/atomic-writes`): **#239** walker writes replace the file whole (temp + rename).
- PR 4 (`fix/worktree-checkout-rollback`): **#245** a worktree add failing in checkout is rolled back.

**Architecture:**

- `git-core`: `stash.rs` (`edit_removed_from_index`), `commit.rs` (`clear_commit_state`),
  `pick.rs`, `merge.rs` (squash cleanup), `worktree_guard.rs` + `beneath_{unix,windows}.rs`
  (`write_with`, `FileInUse`), `worktree.rs` (`AddFailed`, `AddFailedFolderLeft`, rollback).

**Tech Stack:** Rust (`git2` 0.21 / libgit2 1.9.6, `libc`, hand-declared `kernel32`/`ntdll` as
already used).

**Spec:** `docs/superpowers/specs/2026-11-03-data-safety-round-2-design.md`. This plan implements
the **provisional** choice of each decision D1–D6. "Decision hooks" at the end lists what changes
for the other options.

---

## Global Constraints

- **Branches:** each PR from `main`; the four run in parallel; open each only after its gate task
  (SZ, CZ, RZ, WZ). Rebase on `main` before merging; CHANGELOG conflicts are resolved by keeping
  both sections.
- **No new dependency. No frontend change.**
- **Tests use real temp-dir repos** (`tempfile` + `git2::Repository::init`); integration tests
  start with `mod common;` (hermetic git). git CLI twins via the file's `git()` helper
  (`tests/stash.rs` l.1137, `tests/worktree.rs` l.273) or a local copy.
- **Platform gating:** permission tests `#[cfg(unix)]`, skip when `libc::geteuid() == 0`; Windows
  walker/worktree tests `#[cfg(windows)]` (CI `windows-latest`, pushed red first); macOS case test
  `#[cfg(target_os = "macos")]`.
- **Never** touch `~/.local/share/com.browsitory.app`, the real config dir or `~/.gnupg`; probes go
  under `target/` and are deleted.
- **New user-visible strings** (exact, spec §3–4): `FileInUse` Display,
  `WorktreeError::AddFailed`, `WorktreeError::AddFailedFolderLeft`.
- **CHANGELOG:** every push touching `crates/` needs a `CHANGELOG.md` change (pre-push hook). Each
  PR's first task adds a stub (PR 4 edits `### Worktree add`); its Z task writes the final text.
- **Commits:** one per task unless noted, `<area>: <what> (#NNN)`, ending with the session's
  `Co-Authored-By`/`Claude-Session` trailers.

## Pre-flight

- [ ] `git status` clean, rebased on `origin/main`; if `main` moved past 41bc70b4, re-check the
  line references below.
- [ ] Baseline: `cargo test --workspace`, `cd frontend && pnpm test -- --run`. Record counts.
- [ ] Pre-push hook linked (CLAUDE.md "Git hooks").

Reconciled with the code at 41bc70b4 on 2026-11-01:

- `stash.rs`: `save_stash` l.165 (route l.196–214), `changed_paths` l.238,
  `save_path_limited_stash` l.269 (status l.297–320), `kept_entry_in_untracked_folder` l.526,
  `entry_in_the_way` l.643. Tests: `with` l.126, `read` l.132, `stash_commit` l.142, `git` l.1137,
  `top_level` l.1146, `stash_all_without_untracked_stashes_an_edit_to_a_file_removed_from_the_index`
  l.1455, `git_stash_push` l.1541, `assert_nothing_stashed` l.~1551.
- `commit.rs`: `cleanup_after_commit` l.432, `commit_with` l.536 (l.559, l.568–570, l.652–665),
  `clear_pick_leftover` l.745. `pick.rs`: l.373, l.392, l.436, l.680, l.780, l.824, l.845, abort
  l.1028. `merge.rs`: squash l.195, `abort_merge` l.712. Tests: `tests/pick.rs` helpers
  `repo_with_foreign_commit` l.778, `leave_a_pick_state_behind` l.810, sequencer test l.1000–1035.
- `worktree_guard.rs` `write_beneath` l.1016; `beneath_unix.rs` `write` l.293, tests l.702–760;
  `beneath_windows.rs` `write` l.926, `create_leaf` l.898, tests `hold_open_like_a_scanner`
  l.1205, junction write l.1328, `make_read_only` l.1549.
- `worktree.rs` l.26–34, l.100–190; `tests/worktree.rs` `committed_repo` l.265, `has_branch` l.281,
  #200 tests l.285–430.

## Pre-flight notes

- git 2.53 probes (2026-11-01) are recorded in the spec's "Current state"; tests repeat them as
  twins.
- S1 (#247, main at 41bc70b4): `d/f` without untracked files: not in the stash, Pop deletes it;
  with keep-index: deleted at stash time, not in the stash; with untracked files (both
  keep-index values): only in `^3`, Pop → `WouldOverwrite` with conflict markers (keep-index:
  `StagedChanges`), stash kept. Top-level `f` with untracked files: in the worktree tree and
  `^3`, Pop → `WouldOverwrite` with markers; without untracked files it already worked. git's
  `pop` fails for every `-u` stash of these ("could not restore untracked files").
- S2: after a keep-index stash and unstaging the removal, Pop is still refused
  (`WouldOverwrite`: libgit2's safe checkout won't write over the deleted file); after also
  discarding the deletion it brings the edit back. Tested as such.

---

## Part S: stash (PR 1, `fix/stash-index-removed`)

### Task S1: characterize what Stash all does with a folder removed from the index (#247)

**Files:** `crates/git-core/tests/stash.rs`; `CHANGELOG.md` (stub `### Stash: edits removed from the index`).

- [ ] Fixtures: `folder_removed_from_index()`: commit `d/f` = "head\n" and `a` = "a\n";
  `index.remove_dir("d")` (or `remove_path("d/f")`), write; `d/f` = "local\n". And
  `file_removed_from_index()`: same with top-level `f`.
- [ ] Characterization tests on `main` (each with a twin: `git rm -r -q --cached d`, edit,
  `git stash push [-u] [-k]`, `git stash pop [--index]`, assertions as in the spec's git table):
  - `stash_all_loses_an_edit_in_a_folder_removed_from_the_index_today` (untracked off, keep-index
    off): stash tree lacks `d/f`; Pop → `Applied`, `d/f` gone or "head";
  - `…_with_keep_index_today`: `d/f` deleted at stash time, stash lacks "local";
  - `…_with_untracked_files_today` (both keep-index values): Pop → `WouldOverwrite` (or
    `Conflicted`), markers in `d/f`, stash kept;
  - `popping_a_file_removed_from_the_index_with_untracked_files_today` (`f`).
- [ ] Record results in "Pre-flight notes"; S2 turns them into the target. Commit
  `stash tests: characterize edits in a folder removed from the index (#247)`.

### Task S2: route such stashes through the path-limited stash (#247, D1-A)

**Files:** `stash.rs`; `tests/stash.rs`.

- [ ] **Failing tests** (replace S1's "today" tests; untracked × keep-index, plus `f` with
  untracked):
  - `stash_all_keeps_an_edit_in_a_folder_removed_from_the_index`: stash tree `d/f` = "local", the
    index commit has no `d/f`, no `^3` entry for `d/f`; after stashing `d/f` = "head", status
    empty; `pop_stash` → `Applied`, `d/f` = "local", index has `d/f` (HEAD's), stash dropped;
  - `…_and_restores_the_removal_with_restore_staged_changes`: `pop_stash(.., restore_index = true)`
    → index without `d/f`, file "local";
  - `…_with_keep_index_deletes_it_and_keeps_it_in_the_stash`: `d/f` gone, stash tree "local";
    Pop → `StagedChanges`, stash kept; after `repo.reset_default(Some(head), ["d/f"])` Pop →
    `Applied` and "local" (if libgit2 refuses instead, assert stash kept with "local" and record);
  - `…_with_untracked_files_pops_cleanly` (`d/f` and `f`): Pop → `Applied`, "local", no markers;
  - `git_stash_pop_index_restores_our_stash` (twin on the same repo after Browsitory's stash);
  - existing `IgnoredOverwritten` and l.1455 tests stay green.
- [ ] **Implement** spec §1 (`edit_removed_from_index`, the route condition, doc comment citing
  stash.c l.391/l.360/l.292).
- [ ] Commit `stash: keep edits to files removed from the index (#247)`.

### Task SZ: docs, changelog, gate (PR 1)

- [ ] `CHANGELOG.md` → `### Stash: edits removed from the index`:
  - `Fixed: Stash all lost your edits to files in a folder you had removed from the index (git rm -r --cached d, without ignoring it): without Include untracked files Pop deleted them, with Keep staged changes they were deleted at once, and with Include untracked files Pop wrote conflict markers and said it would overwrite local changes. The edits are now stashed and Pop brings them back; with Restore staged changes the removal from the index comes back too (#247).`
- [ ] `docs/ARCHITECTURE.md` "git2 API gotchas" l.148–151 (why libgit2 misses the folder case and
  double-records with untracked files; the route; deliberate difference: recorded once, in the
  worktree tree). `docs/USER_GUIDE.md` "Stashes" l.537 (one sentence).
- [ ] Gate: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, `check-changelog.py`. PR: D1 answer, S1 findings; "Closes #247".

---

## Part C: terminal cherry-picks (PR 2, `fix/keep-sequencer`)

### Task C1: characterize finishing a terminal sequence's step (#241)

**Files:** `crates/git-core/tests/pick_sequence.rs`; `CHANGELOG.md` (stub `### Terminal cherry-picks`).

- [ ] Fixture `terminal_sequence(last: bool)`: base `f`; side branch commits A (`f` = "A"), B
  (`b`), C (`c`); main changes `f`; `git cherry-pick A B C` (or, `last`, `B A`) via the git CLI →
  paused on A; write `f` = "resolved", stage it. Twin helper `paused_twin(..)`.
- [ ] `committing_a_terminal_sequence_step_deletes_its_todo_today`: `commit::commit` →
  `sequencer/` gone; twin `git commit --no-edit` keeps it and `git cherry-pick --continue` applies
  B and C.
- [ ] Commit `pick tests: characterize committing a terminal sequence's step (#241)`.

### Task C2: git's post-commit cleanup (#241, D2-A)

**Files:** `commit.rs`, `pick.rs`, `merge.rs`; `tests/pick_sequence.rs`, `tests/pick.rs`, `tests/hooks.rs`.

- [ ] **Failing tests:**
  - `committing_a_terminal_sequence_step_keeps_its_todo` (replaces C1's): `sequencer/todo`
    bytes unchanged, no `CHERRY_PICK_HEAD`/`MERGE_MSG`, `repo.state() == Clean`, HEAD author =
    A's; then `git cherry-pick --continue` succeeds, log = C, B, resolved A;
  - `committing_the_last_step_of_a_terminal_sequence_ends_it` (`last`): no `sequencer/`,
    `git status --porcelain=v2 --branch` shows no operation; twin agrees;
  - `committing_a_terminal_revert_sequence_step_keeps_its_todo` (`git revert --no-edit A B`);
  - `a_single_cherry_pick_keeps_a_terminal_sequences_todo`: after the step is committed by hand,
    `pick::cherry_pick` of another commit → `sequencer/todo` unchanged (git twin keeps it); same
    for `revert` and a squash `start_merge`;
  - `tests/hooks.rs` `committing_a_merge_during_a_bisect_keeps_the_bisect`: `git bisect start`,
    conflicted merge resolved, `commit` → `BISECT_LOG` kept (twin keeps it);
  - existing `a_leftover_in_a_terminal_sequence_keeps_the_sequencer_todo` (tests/pick.rs l.1005),
    leftover, sequence and abort tests stay green.
- [ ] **Implement** spec §2 (`clear_commit_state`; `cleanup_after_commit`, `clear_pick_leftover`,
  merge leftover l.559, pick.rs l.373/l.392/l.680/l.780/l.824, merge.rs l.195). Abort and rebase
  unchanged.
- [ ] Commit `commit, pick: keep a terminal cherry-pick's remaining steps after a commit (#241)`.

### Task CZ: docs, changelog, gate (PR 2)

- [ ] `CHANGELOG.md` → `### Terminal cherry-picks`:
  - `Fixed: when a cherry-pick or revert of several commits started in a terminal paused on a conflict, committing the resolution in Browsitory dropped the remaining commits. They are now kept, as with git commit, so git cherry-pick --continue (or git revert --continue) carries on; the list is removed after its last commit. A single cherry-pick, revert or squash merge in Browsitory no longer drops such a list either (#241).`
  - `Fixed: committing a merge or cherry-pick no longer ends a git bisect started in a terminal (#241).`
- [ ] `docs/ARCHITECTURE.md` "Cherry-pick sequences" l.515–530 and "Git hooks" l.671–690
  (`clear_commit_state`; libgit2's `state_files`; Abort and rebase keep `cleanup_state`);
  `docs/USER_GUIDE.md` "Rewriting and moving commits" l.700–725 (terminal sequences).
- [ ] Gate: fmt, clippy, `cargo test --workspace`, `check-changelog.py`. PR: D2 answer; "Closes #241".

---

## Part R: safe file writes (PR 3, `fix/atomic-writes`)

### Task R1: a fill seam and the Unix characterization (#239)

**Files:** `worktree_guard/beneath_unix.rs`, `worktree_guard/beneath_windows.rs` (seam only),
`worktree_guard.rs`; `CHANGELOG.md` (stub `### Safe file writes`).

- [ ] **Refactor (no behaviour change):** both walkers get `write_with(workdir, rel, step, fill:
  &mut dyn FnMut(&mut File) -> io::Result<()>)`; `write` passes `write_all`. Existing tests green.
- [ ] Characterization (`beneath_unix` tests):
  `a_failed_write_empties_the_file_today`: `d/f` = "old content"; `write_with` with a fill that
  writes "par" then returns `Err(io::Error::other("disk full"))` → error, file = "par".
- [ ] Commit `walker: a fill seam for writes; characterize a failed write (#239)`.

### Task R2: Unix temp file + rename (#239)

**Files:** `beneath_unix.rs`, `worktree_guard.rs`.

- [ ] **Failing tests** (`beneath_unix`):
  - `a_failed_write_keeps_the_old_content` (replaces R1's): "old content", mode 0o640 kept, the
    folder holds only `f`;
  - `a_failed_write_of_a_new_file_leaves_nothing`;
  - `write_replaces_a_hardlink_instead_of_writing_through_it`: `d/f` hardlinked to `outside/g` →
    after `write`, `d/f` new, `outside/g` old;
  - `write_on_a_fifo_errors_without_blocking` extended: the FIFO is still a FIFO;
  - `write_replaces_a_link_leaf_and_leaves_its_target` (existing behaviour, through rename);
  - `#[cfg(target_os = "macos")] write_keeps_the_on_disk_name_case` (`F.txt`, write `f.txt`);
  - existing exec-bit, read-only (#209), swap and race tests green.
- [ ] **Implement** spec §3 Unix part.
- [ ] Commit `walker: replace a file whole on Unix, never truncate it first (#239)`.

### Task R3: Windows tests first (red on CI) (#239)

**Files:** `beneath_windows.rs`.

- [ ] Tests (`#[cfg(windows)]`): `a_failed_write_keeps_the_old_content`,
  `a_failed_write_of_a_new_file_leaves_nothing`, `write_keeps_the_hidden_attribute`,
  `write_keeps_the_on_disk_name_case`, `writing_a_file_held_without_share_delete_is_in_use`
  (`OpenOptions::share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)`; `is_in_use`, content
  unchanged, no temp left), `a_file_held_open_by_a_scanner_is_still_replaced`
  (`hold_open_like_a_scanner` l.1205).
- [ ] Push the branch; record the failing `windows-latest` job id in "Pre-flight notes" (expected:
  the failed-write and in-use tests red). Commit `walker tests: Windows writes replace whole (#239)`.

### Task R4: Windows temp file + relative rename (#239, D3-A)

**Files:** `beneath_windows.rs`, `worktree_guard.rs`.

- [ ] **Implement** spec §3 Windows part (`NtSetInformationFile` declaration, `RenameInfo` buffer
  builder with a layout test, `FileInUse`). `FileRenameInformationEx` fallback covered by a
  seam test like `delete_classic_with`.
- [ ] Push; R3 tests and the junction (l.1328), SMB and read-only tests green; record the job id.
- [ ] Commit `walker: replace a file whole on Windows, never truncate it first (#239)`.

### Task R5: integration (#239)

**Files:** `tests/merge.rs`, `tests/discard_ignore.rs`.

- [ ] Tests: `resolving_a_conflict_leaves_no_temporary_file` (Unix also: 0o755 kept) and
  `taking_a_side_of_an_add_delete_conflict_leaves_no_temporary_file`;
  `ignoring_a_file_leaves_no_temporary_file`. Assert the folder listing and an empty untracked
  status afterwards.
- [ ] Commit `conflicts, ignore: tests for whole-file writes (#239)`.

### Task RZ: docs, changelog, gate (PR 3)

- [ ] `CHANGELOG.md` → `### Safe file writes`:
  - `Fixed: if writing a resolved conflict or .gitignore failed partway (for example a full disk), the file was left empty or half-written. Browsitory now writes a new copy next to it and swaps it in only when it is complete, so a failure leaves the file as it was (#239).`
  - `Windows: if another program keeps the file open so it can't be swapped, Browsitory now says "'f.txt' is open in another program. Close it, then try again." and changes nothing (#239).`
- [ ] `docs/ARCHITECTURE.md` "Worktree writes and symlinks" l.744–800 (temp + `renameat` /
  relative `FileRenameInformationEx`; what is kept and not; `FileInUse`; temp naming), l.900
  (hardlinks: writes no longer write through). `docs/USER_GUIDE.md` l.113 and l.149 (in-use
  message).
- [ ] Gate: fmt, clippy, `cargo test --workspace`; Windows and macOS CI green (job ids in the PR);
  `check-changelog.py`. PR: D3 answer; "Closes #239".

---

## Part W: worktree checkout rollback (PR 4, `fix/worktree-checkout-rollback`)

### Task W1: characterize a failed checkout (#245)

**Files:** `crates/git-core/tests/worktree.rs`; `CHANGELOG.md` (edit `### Worktree add`).

- [ ] Fixture `branch_with_unwritable_name(repo, name)`: `TreeBuilder` on HEAD's tree inserting a
  blob under a 300-byte name; commit; branch `bad`.
- [ ] `a_checkout_failure_leaves_a_registered_worktree_today`: `create_worktree(.., "wt", dst,
  "newb", Some("bad"))` → `Err(WorktreeError::Git)`; `.git/worktrees/wt/gitdir` exists, `dst`
  exists, `newb` exists and `Branch::delete` fails; twin `git worktree add -b newb <twin_dst> bad`
  → no `<twin_dst>`, no `.git/worktrees/<name>`, `newb` kept.
- [ ] Commit `worktree tests: characterize a failed checkout (#245)`.

### Task W2: roll back after `gitdir`, plain message (#245, D4-A, D5-A)

**Files:** `worktree.rs`; `tests/worktree.rs`.

- [ ] **Failing tests:**
  - `a_checkout_failure_is_rolled_back` (replaces W1's): `AddFailed`, message starts with
    "Couldn't create the worktree: ", not "git operation failed"; no `.git/worktrees/wt`, no `dst`,
    no `newb`, `list_worktrees` has the main one only; retry `create_worktree(.., "wt", dst,
    "good", Some("HEAD"))` → Ok;
  - `a_checkout_failure_on_an_existing_branch_frees_it`: branch `bad` itself → `AddFailed`, `bad`
    kept and deletable;
  - `a_checkout_failure_in_missing_parents_removes_them` (with #200's parents);
  - `#[cfg(unix)] a_foreign_file_in_the_new_folder_is_kept` (skip as root): make the checkout fail
    on a path whose folder the test pre-creates read-only via the tree layout, plus a foreign
    file planted through a `Step`-free approach: create `dst/zzz-foreign` in a `post-checkout`-free
    way by making the tree's first file a folder `dst/a/` that the test can't predict — if no
    deterministic way exists, unit-test the rollback helper directly
    (`rollback_destination(path, tree)` `#[doc(hidden)]`) with a foreign file → folder kept, error
    `AddFailedFolderLeft`, tree files gone;
  - `an_admin_folder_naming_another_destination_is_left_alone`: pre-made `worktrees/wt` with a
    `gitdir` for another path → kept (existing l.403 test style);
  - existing #200 tests green.
- [ ] **Implement** spec §4.
- [ ] Commit `worktree: roll back an add that fails while checking out (#245)`.

### Task W3: Windows names (CI, red first) (#245)

- [ ] `#[cfg(windows)]` `a_reserved_name_in_the_branch_is_rolled_back` (`aux`) and
  `a_trailing_dot_in_the_branch_is_rolled_back` (`a.`): push before W2 lands on the branch if
  practical (red: entry left), else record that W2 already covers them; green after W2. Record job
  ids.
- [ ] Commit `worktree tests: Windows names that can't be checked out (#245)`.

### Task WZ: docs, changelog, gate (PR 4)

- [ ] `CHANGELOG.md` `### Worktree add`: replace "A failure later, while the files are checked out,
  still leaves the worktree and branch; delete the folder, prune, and delete the branch (#200)."
  with:
  - `Fixed: a worktree add that failed while checking out the files (a full disk, or on Windows a file name such as aux or a path that's too long) left the worktree registered with its branch, so it could neither be added again nor removed. Browsitory now removes the files it checked out, the folder, the worktree entry and the new branch, and says why it failed instead of "git operation failed"; if something else is in the folder, the folder is kept and named (#245).`
- [ ] `docs/ARCHITECTURE.md` l.878–889 (rollback after `gitdir`: ownership check, tree-paths-only
  removal, order, residual risk). `docs/USER_GUIDE.md` l.587 (failures are cleaned up; a worktree
  left by an older version: delete the folder, Prune, delete the branch).
- [ ] Gate: fmt, clippy, `cargo test --workspace`; Windows CI green (job id);
  `check-changelog.py`. PR: D4/D5 answers; "Closes #245".

---

## Decision hooks

| Decision | If the user picks… | Change |
|---|---|---|
| D1-B | refuse | S2: `StashError::EditRemovedFromIndex(path)` ("Stashing can't keep your edit to '{0}', which you removed from the index. Add it back with Stage, or commit its removal first.") from `entry_in_the_way` for `INDEX_DELETED|WT_NEW` non-folders, in all modes; no routing; tests expect the refusal with nothing stashed. |
| D2-B | commit box only | C2: only `cleanup_after_commit` and `clear_pick_leftover` use `clear_commit_state`; drop the single-pick/squash/bisect tests (keep as characterization of the remaining behaviour); unrelated finding filed. |
| D3-B | in-place fallback | R4: on `ERROR_SHARING_VIOLATION` delete the temp and run today's in-place write through `reopen_same`; the in-use test expects success; `FileInUse` not added. |
| D4-B | whole folder | W2: rollback = `std::fs::remove_dir_all(path)` (no-follow in std) when the admin folder is ours; drop the foreign-file test and `AddFailedFolderLeft`. |
| D4-C | no rollback | W2: only the `AddFailed` mapping; USER_GUIDE keeps the manual steps. |
| D5-B | Remove anyway | New Part F after W: `remove_worktree(.., force: bool)` (prune `valid + working_tree` even when dirty; still refuses locked/main), `RepoClient.removeWorktree(path, force)` in both clients and the sidecar dispatch, WorktreePanel confirmation ("Remove '…' and discard its uncommitted changes?"), Vitest + one E2E step; touches frontend and transport files (serialize after PR 4). |
| D6-B | three PRs | Parts C and W on `fix/terminal-pick-and-worktree`, two CHANGELOG sections. |
| D6-C | one PR | All parts on `fix/data-safety-round-2`. |

Note on W2's foreign-file test: there is no deterministic way to plant a file in the new folder during libgit2's add from an integration test. The plan therefore falls back to unit-testing a `#[doc(hidden)]` rollback helper. The implementer should pick that route directly.

---
