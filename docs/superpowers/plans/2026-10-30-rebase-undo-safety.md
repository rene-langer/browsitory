# Rebase/Undo Safety Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close five issues:

- **#170:** a rebase start refuses tracked local changes, so rollback and Abort can't destroy
  them.
- **#171:** a paused sequence protects its branch and refuses stash save.
- **#172:** an advisory state lock, forced reloads, fresh markers and unique temp names for the
  undo store shared by two processes.
- **#161:** aborts may restore a tree holding a case collision.
- **#167:** collisions HEAD already holds aren't treated as "brought in".

**Architecture:**

- In `git-core`:
  - `rebase::start_rebase` gains a `local_changes` check;
  - `worktree_guard` gains `find_case_collisions_in`, `known_collisions` and `*_known`
    variants with an `also_known` tree;
  - `rebase` and `pick` aborts pass their original tree.
- In `repo-service::worker`:
  - new `atomic_file.rs` and `state_lock.rs`;
  - `UndoRecorder` writes only under the lock after a forced reload;
  - the worker loop holds the lock across `locks_state` commands;
  - `gate.rs` gets `GateScope` and mtime markers.

**Tech Stack:** Rust (`git2` 0.21 / libgit2 1.9, std `File::lock` since 1.89, `serde_json`),
WebdriverIO + tauri-driver.

**Spec:** `docs/superpowers/specs/2026-10-30-rebase-undo-safety-design.md`. This plan implements
the **provisional** choice of each decision D1–D10. "Decision hooks" at the end lists what
changes for the other options.

---

## Global Constraints

- **Branch:** `fix/rebase-undo-safety` (already pushed with the spec and plan). One PR, opened
  only after Task Z2 passes.
- **No new dependency.** No `docs/LICENSE_COMPLIANCE.md` change (D5-A uses std).
- **Tests use real temp-dir repos** (`tempfile` + `git2::Repository::init`), never a mocked
  `Repository`. A new integration test binary starts with `test_support::hermetic_git!()` (or
  `mod common;`). Repo helpers pin `commit.gpgSign=false` and read the branch from `HEAD`
  (never `init.defaultBranch`).
- **Platform gating:**
  - Case-collision tests that need HEAD to hold the pair switch to it with
    `core.ignorecase=false` first: `#[cfg(target_os = "linux")]`, like the existing ones.
  - Tests that use `gpg.program=/bin/false` are `#[cfg(unix)]`.
  - Lock tests run everywhere.
- **Simulating two processes:** two `Worker`s on one path, or two `UndoRecorder`s, in one test
  process. The lock excludes separate `File` handles in one process (verified by probe). The
  sidecar test (Task D7) uses two real processes.
- **No CLI git in Rust tests unless unavoidable.** Simulate terminal commands with libgit2:
  - `git merge --abort` = `cleanup_state` + `reset(HEAD, Hard)`;
  - `git switch` = `set_head` + forced `checkout_head`.
- **Never** touch `~/.local/share/com.browsitory.app` or the real config dir. Probes go under
  `target/` and are deleted afterwards.
- **Error copy is exact** (spec §1, D2, D7). Put it in `const`s and assert it verbatim once.
- **CHANGELOG:** every push touching `crates/`, `frontend/src/` or `e2e/` needs a
  `CHANGELOG.md` change (pre-push hook). Task A1 adds a stub line under `## [Unreleased]` →
  `### Rebase/undo safety`, and Task Z1 writes the final text.
- **Commits:** one per task unless noted, with message `<area>: <what> (#NNN)`. Each ends with the
  session's `Co-Authored-By`/`Claude-Session` trailers.

## Pre-flight

- [ ] `git status` is clean on `fix/rebase-undo-safety`, rebased on `origin/main` if `main`
  moved. If it did, re-check the line references below.
- [ ] `cargo test -p git-core && cargo test -p repo-service` passes before any change. This is
  the baseline; record the counts in "Pre-flight notes" below.
- [ ] Link the pre-push hook (CLAUDE.md "Git hooks") if this worktree doesn't have it.

Reconciled with the code at 6044d7bc on 2026-10-30:

- `crates/git-core/src/worktree_guard.rs`:
  - `find_case_collision_in` (l. 524) returns only the first collision;
  - `prepare_forced_checkout` (l. 593) has the HEAD exemption inline (l. 605–620);
  - `refuse_linked_folders` (l. 671) checks collisions at l. 716–731 with no exemption.
- `crates/git-core/src/rebase.rs`:
  - `start_rebase` (l. 211) checks out onto at l. 239–243;
  - the rollback is at l. 258–266;
  - `abort_rebase` is at l. 804, and `restore_original_branch(repo, branch_ref)` at l. 845.
- `crates/git-core/src/pick.rs`: `abort_sequence`'s rewind is at l. 767–775 and uses
  `refuse_linked_folders(Checkout(&tree))`.
- `refuse_linked_folders` callers:
  - `merge.rs:45,49`, `pick.rs:277,771`, `rebase.rs:239,551`, `branch.rs:94,110`;
  - `remote.rs:906`, `stash.rs:516`, `submodule.rs:206`.
- `prepare_forced_checkout` callers: `merge.rs:424,549`, `stage.rs:149`, `undo.rs:557`,
  `reset.rs:40`, `rebase.rs:851`.
- `crates/repo-service/src/worker/`:
  - `mod.rs` worker loop l. 735–1500 (settle l. 777, `track` l. 779, gate l. 795–798, finish
    l. 1491);
  - `undo.rs` `settle` l. 225, `finish` l. 258, `step` l. 352;
  - `undo_store.rs` temp name l. 192 and `reload_if_changed` l. 163;
  - `sequence_store.rs` temp name l. 156;
  - `rebase_store.rs` temp name l. 228–233;
  - `gate.rs` `gate_during_operation` l. 23 and `marker` l. 112.
- The probe results that the spec quotes (#170 cases 1–4, lock semantics) are recorded in the
  spec's "Current state". The probe sources were deleted.

## Pre-flight notes

Baseline at 170fd967 (2026-10-30): `cargo test -p git-core -p repo-service` passed 1220, failed
0, ignored 4. The pre-push hook was already linked. Toolchain: `rust-toolchain.toml` pins
`stable` (local 1.97.1), CI uses `dtolnay/rust-toolchain@stable`, and no crate declares a
`rust-version`, so std's `File::lock` (1.89) needs no MSRV change (D5-A).

Decisions: D1-A, D2-A, D3-A, D6-A, D9-A (confirmed by the user); D4-A, D5-A, D7-A, D8-A, D10-A
(recommended, taken).

Where the code won over the plan, or the plan was extended:

- **A1/A2:** the `*_known` variants (`refuse_linked_folders_known`,
  `prepare_forced_checkout_known`) landed in A1 together with `known_collisions`, since A1's
  `refuse_linked_folders` change is the same code; A2 only wires the aborts to them. A2's unit
  test `also_known_exempts_only_its_own_collisions` therefore never failed on its own; the A2
  integration tests did (`CaseCollision`). `first_collision_across` is now test-only.
- **A1 tests:** `head_with_pair_at` detaches HEAD before moving the branch, because
  `switch_branch` to the branch already checked out is a no-op. The non-ff merge test asserts
  `MergeOutcome::Merged` and the merged file (a non-ff merge leaves the merge for the user to
  commit, so there is no two-parent commit yet). The stash test saves the stash with
  `core.ignorecase=false`: libgit2's stash diff fails on Linux with both `D` and `d/` on disk
  under a simulated `ignorecase` ("attempt to merge diffs created with conflicting options").
  The apply, which is what the guard checks, runs folded. Added a unit test
  `a_collision_head_already_holds_is_not_brought_in` next to the spelling one.
- **A2 test fixture:** the sequence test has a local commit on `main` so the second pick
  conflicts; it asserts HEAD == that original commit rather than `evil`.
- **A3:** rows start from a raw forced checkout of `evil` (`head_on_pair`), then reset the
  outside folder, so they measure only the git-core operation. The "changes `d/x`" row merges
  with `--no-ff`.
- **B1:** no existing test started a rebase on a dirty tracked tree (`cargo test --workspace`
  stayed green after the refusal), so no test needed fixing. Added
  `start_lists_every_changed_path_sorted`. The ignored-file test uses `.git/info/exclude`, so the
  ignore rule itself isn't a tracked change.
- **Undo labels (two bugs from "Unrelated findings"), new commit after B2:**
  - `Rebase onto <onto>` shortens `onto` to 7 characters when it is a full 40/64-hex id (the
    frontend's "Rebase onto here" sends one) and keeps a ref name as it is.
  - A failed `StartRebase` that still changed the repository keeps its entry (it is what makes
    the loss recoverable) under "Rebase onto <id> failed (rolled back)", or "Rebase onto <id>
    failed" when HEAD is still detached afterwards (the rollback itself failed). This needed
    `worker::rebase::start` to return its result so the worker can relabel before replying.
    With #170 a failed start from a clean tree usually changes nothing and records no entry.
- **B3:** each spec file gets a fresh fixture clone (`beforeSession` → `resetFixtureRepo`), so
  the shared-repo risk is per file only. Both `rebase.spec.ts` and
  `restart-resilience.spec.ts` get a `before` guard (`git status --porcelain
  --untracked-files=no --ignore-submodules`). The new case restores the file in a `finally`.
  The alert is matched by XPath (`//p[@role='alert'][contains(., …)]`).
- **C1:** the gate unit tests save sequences of real commits (`SequenceStore::load` validates
  that the commits exist).
- **D3:** `exclusive()` returns the guard it took (or none when the caller's guard or an
  unsupported lock applies) and always force-reloads. `relabel` of a pending entry is kept in
  `pending_label` and applied when `finish` records it, because the forced reload before the
  write would otherwise drop it (the sequence "Cherry-pick N commits" label). `record` no longer
  reloads itself; its callers have just reloaded under the lock. `try_acquire` is test-only.
- **D4:** a `locks_state` command that gets `Busy` after `LOCK_WAIT` runs with its undo plan
  dropped (`unrecorded`), so it isn't recorded (D6-A); Undo/Redo reply `UNDO_BUSY` without
  running (`undo::reply_busy`). The pause hook is `Worker::spawn_with_pause_before_finish`, an
  uninhabited `PauseHook` outside tests. The worker tests live in a separate
  `worker::state_lock_tests` module. The race test's `onto` is a commit off the branch: with
  `onto` = the step's own parent, the rebased commit can be byte-identical within one second and
  nothing is recorded. The race test was checked red with the loop's lock disabled. Added
  `pick_and_revert_markers_carry_their_state_files_mtime` in `gate.rs`.
- **D5:** the two-sidecar test passes without D3/D4 as well (it runs sequentially; no window
  for the race). It stays as a smoke test of the cross-process path.
- **CI:** clippy on macOS/Windows flagged the Linux-only `case_collision.rs` helpers as dead
  code; they carry `#[cfg_attr(not(target_os = "linux"), allow(dead_code))]`. The `audit` job
  failed on main too (RUSTSEC-2026-0418, libz-sys 1.1.29, published 2026-10-28); a separate
  commit bumps the lockfile to libz-sys 1.1.31 (same license, no new dependency).
- **Review fixes** (independent review of `main...HEAD`):
  - the forced reloads no longer replace the in-memory stacks after a failed save (`unsaved`),
    which kept undo working for the session on main (`a_failed_save_keeps_the_session_history`);
  - `settle` clears a relabel left over from a command that didn't `finish`, so it can't rename
    another window's pending entry (`a_relabel_with_nothing_pending_never_labels_a_later_entry`);
  - after one `LOCK_WAIT` timeout, later locked commands try once until the lock is taken again
    (`only_the_first_locked_command_waits_while_the_lock_stays_held`).
- **M4 (user chose option C):** while the lock stays busy past the wait, the commands that
  destroy work-tree content with no other recovery are refused with `DISCARD_BUSY` ("Another
  Browsitory window is changing this repository's undo history. Try again in a moment, so this
  change can be undone."): `DiscardHunk`, `DiscardLines`, `DiscardFile`, `DiscardAllUnstaged`
  and `ResetToCommit { mode: Hard }`. Everything else runs unrecorded as before.
  `RestoreReflogEntry`, branch/tag changes and soft/mixed resets only move refs or the index
  (recoverable from the reflog, the work tree kept); switch, checkout, merge, picks, pull and
  stash apply refuse to overwrite local changes; `IgnorePath` only appends to `.gitignore`.
  Test: `work_tree_destroying_commands_are_refused_while_the_lock_stays_held`.

---

## Part A: case collisions (#167, #161), git-core only

### Task A1: every collision across sources, and a HEAD exemption in `refuse_linked_folders` (#167)

**Files:**

- `crates/git-core/src/worktree_guard.rs`
- `crates/git-core/tests/case_collision.rs`
- `CHANGELOG.md` (stub)

**TDD requirement.** Write these tests first; they must fail.

- `worktree_guard.rs` `#[cfg(test)]`:
  - `find_case_collisions_in_returns_every_collision_across_sources`: two sources, each with a
    pair (`D`+`d/x` in source 0, `Z`+`z/x` in source 1), plus a cross-source pair (link `K` in
    0, `k/x` in 1). All three come back, in entry order.
  - `a_collision_spelled_differently_from_heads_is_not_known`: HEAD has link `Db` + `dB/x`.
    Incoming has link `Db` + `DB/x`. The collision `(Db, DB)` is reported.
- `tests/case_collision.rs`. Add a helper `head_with_pair(fx) -> Oid`: switch to `evil` with
  `ignorecase=false`, then set it back to `true` (the pattern of
  `a_head_with_a_collision_can_still_be_reset_and_discarded`). Then add these
  `#[cfg(target_os = "linux")]` tests:
  - `a_head_with_a_collision_merges_an_unrelated_branch`: branch `side` = `evil` + `b.txt`;
    `main` at `evil` + `c.txt`; non-ff `merge::start_merge` → `Merged`/committed, and `b.txt`
    exists.
  - `a_head_with_a_collision_fast_forwards_to_an_unrelated_descendant`: `side` = `evil` +
    `b.txt`; ff merge → HEAD == side.
  - `a_head_with_a_collision_cherry_picks_an_unrelated_commit`.
  - `a_head_with_a_collision_reverts_an_unrelated_commit`.
  - `a_head_with_a_collision_applies_an_unrelated_stash`: edit `a.txt`, `save_stash`,
    `apply_stash(0)` → `a.txt` edited again.
  - `a_head_with_a_collision_pulls_fast_forward`: reuse the existing pull fixture pattern from
    `pull_fast_forward_to_a_case_collision_is_refused`.
  - `a_head_with_a_collision_rebases_onto_an_unrelated_commit`: plan `[Pick b]` onto a commit
    that keeps the pair → `Done`.
  - `switching_between_branches_holding_the_same_collision_is_allowed`: from `evil` to `evil2`
    (`with_pair(.., "PWNED2\n")`) → Ok. `d/x` = `PWNED2`. This is a behaviour change (spec §4).
  - `a_merge_bringing_a_second_collision_is_refused_even_when_head_holds_one`: `Z`+`z/x` →
    `MergeError::CaseCollision((Z, z))`, and nothing written.
  - `a_cherry_pick_bringing_a_second_collision_is_refused_even_when_head_holds_one`.

**Implementation:**

- `fn find_case_collisions_in` builds entries as `find_case_collision_in` does, then calls
  `collisions_across(entries, true)`. `find_case_collision_in` keeps its signature (clone and
  submodule use the single-source wrapper).
- `fn known_collisions(repo, paths, also_known: Option<&git2::Tree>)` returns HEAD tree's
  `find_case_collisions` plus `also_known`'s. Use it in `prepare_forced_checkout`, replacing
  the inline block (behaviour unchanged; the existing reset/discard tests keep passing).
- `refuse_linked_folders`: `if let Some(c) = find_case_collisions_in(&sources,
  ALL)?.into_iter().find(|c| !known.contains(c)) { return Err(c.into()) }`, where `known` is
  computed only when the list is non-empty. Rewrite the comment at l. 716–719 to say that HEAD's
  identical collisions are exempt and that the link scan above stays the guard.

- [ ] Tests → fail (the unit tests fail to compile; the integration ones fail with
  `CaseCollision`).
- [ ] Implement → `cargo test -p git-core --test case_collision` and `cargo test -p git-core
  worktree_guard` pass. The existing `a_repo_whose_head_has_a_collision_still_switches_to_a_clean_branch`
  still refuses `evil2` (HEAD = `clean` there).
- [ ] Add the CHANGELOG stub: `## [Unreleased]` → `### Rebase/undo safety` → `- (in progress)`.
- [ ] Commit `git-core: collisions HEAD already holds don't block unrelated operations (#167)`.

### Task A2: `also_known` for aborts (#161)

**Files:**

- `crates/git-core/src/worktree_guard.rs`
- `crates/git-core/src/rebase.rs`
- `crates/git-core/src/pick.rs`
- `crates/git-core/tests/case_collision.rs`

**TDD requirement** (`#[cfg(target_os = "linux")]`, written first, failing with
`CaseCollision` today):

- `rebase_abort_restores_an_original_tree_holding_a_collision`:
  - Fixture: `main = evil → e` (`e` edits `a.txt`; the tip holds the pair), checked out via
    `head_with_pair`. `onto = evil → rm` (`rm` deletes `D`).
  - Rebase onto `rm` with plan `[Edit e]`. HEAD is now detached at `rm + e`, without the pair.
    The onto checkout and the step bring no collision in.
  - `abort_rebase` → `restored: true`, HEAD on `main`, `D` is a link and `d/x` exists. Today
    this is refused: the target, `main`'s tree, holds the pair and HEAD doesn't.
- `sequence_abort_rewinds_to_an_original_tree_holding_a_collision`:
  - HEAD = `main` at `evil`.
  - `pick::cherry_pick_many([rm, conflicting])`, where `rm` deletes `D` and `conflicting` edits
    `a.txt` against a local commit, so the sequence stops on a conflict.
  - `abort_sequence` → `rewound: true`, HEAD == `evil`, and the pair is back.
- `#[cfg(unix)]` `a_failed_rebase_start_rolls_back_to_an_original_tree_holding_a_collision`:
  HEAD at `evil`, onto `rm`, `commit.gpgSign=true`, `gpg.program=/bin/false`, plan `[Pick e]`.
  Start → `Err(Signing…)`, HEAD on `main`, and the pair is back.
- `worktree_guard.rs` unit test `also_known_exempts_only_its_own_collisions`: HEAD without a
  pair, target with `D`+`d/x` and `Z`+`z/x`, `also_known` with `D`+`d/x` only →
  `prepare_forced_checkout_known` refuses `(Z, z)`.

**Implementation:**

- Add `refuse_linked_folders_known(repo, incoming, also_known)` and
  `prepare_forced_checkout_known(repo, target, paths, also_known)`. The old names forward with
  `None`.
- `restore_original_branch(repo, branch_ref, original_tip: Oid)`: pass
  `Some(&repo.find_commit(original_tip)?.tree()?)`. Update both callers (rollback:
  `state.original_tip`; `abort_rebase`: `state.original_tip`).
- `abort_sequence` rewind: `refuse_linked_folders_known(repo, &[Checkout(&tree)],
  Some(&tree))`.
- Doc comments: "restores a tree the user already had (#161); its collisions are not brought
  in".

- [ ] Tests → fail. Implement → pass. Run `cargo test -p git-core --test rebase --test
  pick_sequence --test symlink_write_through` too (they call these functions).
- [ ] Commit `git-core: rebase and sequence aborts may restore a tree holding a case collision (#161)`.

### Task A3: case-insensitive CI probe rows (#167)

**File:** `crates/git-core/tests/case_insensitive_links.rs`

**TDD requirement:** this probe self-skips on Linux, so "red" can't be shown locally. Run it
with `BROWSITORY_CASE_PROBE_FORCE=1` to prove the harness, and rely on CI (macOS/Windows) for
the real file system.

- [ ] In `run_rows`, add rows that start from "HEAD holds the pair, checked out by raw
  libgit2's forced checkout of `evil`" (use `checkout_two_step` or a forced `checkout_tree` +
  `set_head`):
  - git-core non-ff merge of an unrelated branch;
  - git-core cherry-pick of an unrelated commit;
  - git-core stash apply of an unrelated stash;
  - git-core merge of a branch that changes `d/x`.
  Each row records `describe(workdir)` and `show(result)`, and the existing
  `outside_intact` assertion covers them.
- [ ] `BROWSITORY_CASE_PROBE_FORCE=1 cargo test -p git-core --test case_insensitive_links -- --nocapture` passes locally.
- [ ] Commit `git-core: case-insensitive probe rows for operations on a HEAD holding a collision (#167)`.
- [ ] After the PR's CI runs: if any new row fails `outside_intact` on macOS/Windows, stop and
  switch to D9-B (see Decision hooks) before merging.

---

## Part B: rebase start refuses local changes (#170)

### Task B1: `RebaseError::LocalChanges` in `start_rebase`

**Files:**

- `crates/git-core/src/rebase.rs`
- `crates/git-core/tests/rebase.rs`
- any other test that starts a rebase on a dirty tree

**TDD requirement.** Write these tests first in `tests/rebase.rs`; they must fail.

- `start_refuses_an_unstaged_change_and_changes_nothing`: dirty `f.txt` (a file that neither
  the onto diff nor any step touches, the probe's case). Start → `Err(LocalChanges { paths:
  ["f.txt"] })`. HEAD is still attached to the branch at the same tip, `f.txt` = the dirty
  content, `repo.state()` is `Clean`, and the index is unchanged.
- `start_refuses_a_staged_change`.
- `start_refuses_a_deleted_tracked_file`.
- `start_allows_untracked_files`: an untracked `notes.txt` → `Done`, and `notes.txt` is
  intact.
- `start_ignores_ignored_files`: an ignored file in `.gitignore` → `Done`.
- `local_changes_copy_names_the_first_path_and_counts_the_rest`: one path → the exact D2-A
  single-path text. Three paths → "…to a.txt and 2 other files. …". Two paths → "…and 1
  other file. …".
- `#[cfg(unix)]` `a_failed_first_step_on_a_clean_tree_rolls_back_to_the_branch`:
  `commit.gpgSign=true`, `gpg.program=/bin/false`, plan `[Pick g]` → `Err`. HEAD is on the
  branch at its tip, there is no `CHERRY_PICK_HEAD`, and status is empty.

**Implementation:**

- `const LOCAL_CHANGES_ONE: &str` / a `fn local_changes_message(paths)` per D2-A.
- `RebaseError::LocalChanges { paths: Vec<String> }`, with `#[error("{}",
  local_changes_message(paths))]`.
- `fn local_changes(repo)` as in spec §1: `StatusOptions` with `include_untracked(false)`,
  `include_ignored(false)`, `exclude_submodules(true)`, `renames_head_to_index(false)`, and the
  flags listed there. Call it right after the detached-HEAD check.
- Run `cargo test -p git-core` and `cargo test -p repo-service`. Fix every existing test that
  now fails because it started a rebase on a dirty tree by committing (or not creating) the
  file. Do **not** weaken the new check. Candidates, by count of `start_rebase` uses:
  - `tests/rebase.rs` (47);
  - `tests/symlink_write_through.rs` (9; untracked links are allowed, tracked edits are not);
  - `worker/mod.rs` (9), `worker/rebase.rs` (3), `worker/gate.rs` (2);
  - `tests/signing_ssh.rs` (2), `tests/case_insensitive_links.rs` (2), `tests/head_info.rs`
    (1), `tests/case_collision.rs` (1);
  - `tauri-app/src/commands/rebase.rs` (2), `commands/mod.rs` (1);
  - `vscode-sidecar/tests/protocol_roundtrip.rs` (3).

  Record each fix in "Pre-flight notes".

- [ ] Tests → fail. Implement → pass. Run `cargo test --workspace`.
- [ ] Commit `git-core: refuse to start a rebase over uncommitted changes (#170)`.

### Task B2: worker-level check

**File:** `crates/repo-service/src/worker/mod.rs` (tests)

**TDD requirement:**

- `a_refused_rebase_start_records_no_undo_entry_and_keeps_the_edit`: worker on a repo with a
  dirty tracked file. `start_rebase` → `Err` whose text equals the D2-A copy.
  `get_undo_state().undo` is unchanged, and the file is intact.
- `a_refused_rebase_start_leaves_no_rebase_stored`: `get_head_info().rebase` is `None`, and
  `<gitdir>/browsitory/rebase.json` doesn't exist.

These pass as soon as B1 lands. Write them to lock the behaviour in; they need no new code.

- [ ] Write the tests → they pass.
- [ ] Commit `repo-service: a refused rebase start records nothing (#170)`.

### Task B3: Tauri E2E (D10-A)

**File:** `e2e/specs/rebase.spec.ts`

- [ ] First check that every existing rebase case starts from a clean tracked tree. The E2E repo
  is shared across specs, and some may leave tracked edits. Add a `before` step in
  `rebase.spec.ts` (and `restart-resilience.spec.ts`) that fails with a clear message if
  `git status --porcelain --untracked-files=no` isn't empty, or that commits or resets
  whatever the spec itself left.
- [ ] Add the case "starting a rebase with uncommitted changes is refused":
  1. Commit a side-branch fixture with the existing helpers.
  2. Modify a tracked file on disk.
  3. Open "Rebase onto here", then "Start rebase".
  4. The error banner contains "You have uncommitted changes to", the planner is still open,
     and the file keeps its content.
  5. Close the planner and restore the file.
- [ ] Run alone: full build sequence from CLAUDE.md, then `pnpm test -- --spec
  ./specs/rebase.spec.ts` (with `xvfb-run -a` if headless).
- [ ] Commit `e2e: starting a rebase over uncommitted changes is refused (#170)`.

---

## Part C: a paused sequence protects its branch (#171)

### Task C1: `GateScope`

**Files:**

- `crates/repo-service/src/worker/gate.rs`
- `crates/repo-service/src/worker/mod.rs`

**TDD requirement.** Write these tests first.

`gate.rs` unit tests:

- `gate_scope_for_a_sequence_on_a_branch_names_it_and_refuses_stash_save`;
- `gate_scope_for_a_detached_sequence_names_no_branch`;
- `gate_scope_for_a_rebase_is_unchanged` (branch = rebased branch, stash save refused);
- `gate_scope_is_none_when_nothing_is_stored`.

Worker tests (`mod.rs`):

- `renaming_the_sequence_branch_is_refused_while_a_sequence_is_paused`:
  1. Run a sequence of two picks; the second conflicts.
  2. `rename_branch(main → other)` → `Err(OperationInProgress text)`.
  3. `abort_sequence` → `rewound: true`.
- `deleting_the_sequence_branch_is_refused_while_a_sequence_is_paused`: after the stop, move
  HEAD with libgit2 (`set_head("refs/heads/side")` + forced checkout), so libgit2's own
  current-branch protection doesn't hide the gate. `delete_branch(main, force=true)` → `Err`.
- `adding_a_worktree_on_the_sequence_branch_is_refused`.
- `stash_save_is_refused_while_a_sequence_is_paused`.
- `other_branches_stay_manageable_during_a_paused_sequence`: create a branch `spare` with
  libgit2 (the worker's `CreateBranch` checks out, so it is gated), then rename it and delete
  it through the worker → Ok.

**Implementation:**

- `pub(super) struct GateScope { branch: Option<String>, refuse_stash_save: bool }`.
- `OperationProbe::gate_scope(&self, repo) -> Option<GateScope>`: rebase first, then sequence
  (`OriginalRef::Branch(name)` → short name).
- `gate_during_operation(command, &scope)`: the branch arms compare with `scope.branch`, and
  `SaveStash` uses `scope.refuse_stash_save`.
- In the worker loop, replace `rebased_branch` + `sequence_store.load(...)` with `if let
  Some(scope) = probe!().gate_scope(&repo) { command = gate::gate_during_operation(command,
  &scope); }`.
- Update the doc comment on `gate_during_operation` (sequence branch, stash save).

- [ ] Tests → fail. Implement → pass (`cargo test -p repo-service gate` and the worker tests
  above).
- [ ] Commit `repo-service: a paused sequence protects its branch and refuses stash save (#171)`.

---

## Part D: undo store concurrency (#172)

### Task D1: `atomic_file::write_atomically`

**Files:**

- `crates/repo-service/src/worker/atomic_file.rs` (new)
- `worker/mod.rs` (`mod atomic_file;`)
- `undo_store.rs`, `sequence_store.rs`, `rebase_store.rs`

**TDD requirement** (unit tests in `atomic_file.rs`, written first):

- `writes_the_bytes_and_leaves_no_temp_file`;
- `temp_names_differ_between_calls_in_one_process`: expose `fn temp_name(file) -> PathBuf` as
  `pub(super)`; two calls → different names, both containing `std::process::id()`;
- `concurrent_writers_in_one_process_never_fail`: 8 threads × 50 writes of distinct JSON to one
  file. Every call is `Ok`, the final file parses, and the directory has no `*.tmp`;
- `a_failed_rename_removes_the_temp_file`: the target is a directory → `Err`, and no `*.tmp`
  remains.

**Implementation:** move `rebase_store`'s counter scheme into `write_atomically`. Replace the
three inline writers. The existing store tests, which check that no temp file is left behind
and that a directory in place of the file fails, must still pass.

- [ ] Tests → fail. Implement → pass (`cargo test -p repo-service store atomic_file`).
- [ ] Commit `repo-service: unique temp names for every state file (#172)`.

### Task D2: `StateLock`

**Files:**

- `crates/repo-service/src/worker/state_lock.rs` (new)
- `worker/mod.rs` (`mod state_lock;`)

**TDD requirement** (unit tests, written first):

- `a_second_handle_is_busy_until_the_first_releases`: two `StateLock::open` on one repo.
  `a.acquire(0)` → `Held`, `b.try_acquire()` → `Busy`, drop `a`'s guard → `b.try_acquire()` →
  `Held`.
- `acquire_waits_then_reports_busy`: holding `a`, `b.acquire(150ms)` returns `Busy` after ≥ 150
  ms.
- `acquire_succeeds_when_the_holder_releases_during_the_wait`: a thread releases after 50 ms,
  and `b.acquire(2s)` → `Held` in < 2 s.
- `a_panicking_holder_thread_releases_the_lock`.
- `a_killed_holder_process_releases_the_lock`. This is a self-exec test, and portable:
  - an `#[ignore]` helper test `lock_holder_child` does nothing unless
    `BROWSITORY_LOCK_HOLDER=<gitdir>` is set; then it acquires, prints `locked`, and sleeps
    60 s;
  - the parent spawns `std::env::current_exe()` with `["--exact",
    "worker::state_lock::tests::lock_holder_child", "--ignored", "--nocapture"]` and the env
    var, then reads stdout until `locked`;
  - it asserts `try_acquire()` → `Busy`, calls `child.kill()` and `wait()`, then asserts
    `try_acquire()` → `Held`.
- `the_lock_file_stays_after_release`.
- `each_linked_worktree_has_its_own_lock`: a lock on the main repo doesn't block one on a
  linked worktree.

**Implementation:** as spec §3.2. Use `std::fs::File::try_lock` and match
`TryLockError::WouldBlock` → `Busy`; `TryLockError::Error(e)` with `ErrorKind::Unsupported` →
`Unavailable(e)`. Back off 10 ms, doubling to 100 ms. `#[cfg(test)] pub(super) const
LOCK_WAIT: Duration = 300ms`, otherwise 10 s.

- [ ] Tests → fail. Implement → pass (`cargo test -p repo-service state_lock`).
- [ ] Commit `repo-service: advisory state lock under <gitdir>/browsitory (#172)`.

### Task D3: recorder writes only under the lock

**Files:**

- `crates/repo-service/src/worker/undo.rs`
- `crates/repo-service/src/worker/undo_store.rs`

**TDD requirement.** Write these in `undo.rs` tests first. Two `UndoRecorder::open` on one repo
stand for the two processes. Markers are passed as strings, as the worker does.

- `settle_keeps_a_pending_entry_while_another_holder_has_the_lock`:
  1. Recorder A creates a pending entry (`begin`, then `finish(marker = Some("merge X"))`).
  2. The test holds the `StateLock`.
  3. B calls `settle(repo, || None)` (the operation looks ended to B).
  4. Release the lock. `undo.json` still has `pending`. A's `finish(None, false)` records it.
     Both A's and B's `state().undo` show the entry.
- `settle_drops_a_stale_pending_entry_when_the_lock_is_free`: the same without holding the lock.
  `pending` is gone (today's behaviour is kept).
- `finish_reloads_before_adding_a_pending_entry`:
  1. B records entry "B1".
  2. A, which loaded before B's save, then makes a pending entry.
  3. `undo.json` has both "B1" and `pending`.
- `recording_in_turn_keeps_both_entries`: A records "A1", B records "B1", A records "A2". The
  stack is `[A1, B1, A2]`, read from either recorder.
- `undo_reports_busy_while_the_lock_stays_held`: the test holds the lock. `undo()` (with
  `LOCK_WAIT` from the test cfg) → `Err(UNDO_BUSY)`, and nothing is restored.
- `undo_store` test `reload_reads_a_same_mtime_rewrite`: write `undo.json`, set the same mtime
  back with `File::set_modified` after a second write, and check that `reload` (not
  `reload_if_changed`) sees the change.

**Implementation** (spec §3.3):

- `UndoStore::reload`.
- `UndoRecorder` owns a `StateLock`.
- New signatures: `settle(&mut self, repo, guard: Option<&StateGuard>, marker: impl Fn() ->
  Option<String>)`, `finish(&mut self, repo, guard, marker, aborted)`, and `undo`/`redo(repo,
  guard, marker)`.
- With a guard, `reload` and act. Without one, take the fast path; if a write is needed,
  `try_acquire()`. `Busy` → skip (`eprintln!` once per recorder). `Unavailable` → act unlocked
  (logged once).
- `const UNDO_BUSY: &str = "Another Browsitory window is changing this repository's undo
  history. Try again in a moment."`

- [ ] Tests → fail. Implement → pass.
- [ ] Commit `repo-service: undo history writes happen under the state lock after a fresh reload (#172)`.

### Task D4: the worker loop holds the lock across commands that can end an operation

**File:** `crates/repo-service/src/worker/mod.rs`

**TDD requirement.** Write these worker tests first.

- `a_rebase_continued_while_another_worker_reads_is_still_one_undo_entry`. This is the
  issue's race, made deterministic:
  - Add `#[cfg(test)] Worker::spawn_with_pause_before_finish(path) -> (Worker, PauseControl)`.
    In the loop, after the command and before `finish`, a `#[cfg(test)]` hook sends "reached"
    and waits for "go". It only fires for commands for which `PauseControl::arm()` was called.
  - Steps:
    1. Worker A (paused) and worker B (plain) on one repo. A starts a rebase with `[Edit a]`.
    2. A arms the hook and calls `rebase_continue` on a thread.
    3. Wait for "reached". `rebase.json` is gone by then.
    4. B runs `get_status()` and `get_head_info()`.
    5. Send "go" and join.
  - A's `get_undo_state().undo` is `Some("Rebase onto …")`, and so is B's.
  - Without the fix, B's settle drops `pending`, and A records nothing.
- `a_merge_restarted_outside_with_the_same_heads_does_not_reuse_the_old_pending_entry`:
  1. A conflicted merge in a worker leaves `pending` with the label "Merge side".
  2. Restart the worker (drop it and every handle).
  3. Simulate `git merge --abort` (cleanup_state + hard reset).
  4. Run the same merge with libgit2 `repo.merge(&[annotated side])`, then
     `File::set_modified(MERGE_HEAD, now + 2 s)` so the mtime differs on any file system.
  5. Spawn the worker, resolve and `commit`.
  6. `get_undo_state().undo` is not "Merge side". The old pending entry was dropped and the
     new merge started outside isn't recorded.
- `a_locked_command_runs_after_the_wait_when_the_lock_stays_held`: the test holds the
  `StateLock`. `stage_file` returns `Ok` after ≥ `LOCK_WAIT`, the file is staged, and the undo
  state is unchanged (unrecorded, D6-A).
- `reads_never_wait_for_the_lock`: the test holds the lock. `get_status` returns in well under
  `LOCK_WAIT` (assert < 200 ms with the 300 ms test wait).
- `undo_replies_busy_while_another_holder_keeps_the_lock`: `undo()` →
  `Err(UNDO_BUSY)`.

**Implementation:**

- `fn locks_state(command: &Command) -> bool`, as in spec §3.3. Add a unit test
  `locks_state_covers_every_recordable_and_operation_ending_command`, which builds one sample
  of each listed variant and one read.
- Loop order:
  1. `let guard = if locks_state(&command) { match state_lock.acquire(LOCK_WAIT) { Held(g) =>
     Some(g), Busy => { log once; None } (Undo/Redo → reply UNDO_BUSY and skip), Unavailable
     => { log once; None } } } else { None };`
  2. `settle(repo, guard.as_ref(), || marker(...))`;
  3. begin, gate, the command;
  4. `#[cfg(test)]` hook;
  5. `finish(repo, guard.as_ref(), || marker(...), aborts)`;
  6. `drop(guard)`;
  7. `held.release()`.
- `OperationProbe::marker` (D8-A): append ` @<ns>` from
  `fs::metadata(repo.path().join(file))?.modified()` for MERGE_HEAD, CHERRY_PICK_HEAD and
  REVERT_HEAD (omitted on error). Update `marker_is_stable_for_the_whole_operation` (its merge
  assertion now has the suffix) and add `a_rewritten_merge_head_changes_the_marker` in
  `gate.rs`.

- [ ] Tests → fail. Implement → pass. Run `cargo test -p repo-service` in full: every
  restart/undo test from #169 must stay green.
- [ ] Commit `repo-service: hold the state lock across commands that can end an operation; fresh merge/pick markers (#172)`.

### Task D5: sidecar two-process round trip

**File:** `crates/vscode-sidecar/tests/protocol_roundtrip.rs`

**TDD requirement:** `a_merge_finished_in_a_second_sidecar_is_one_undo_entry`.

1. Spawn two sidecar processes with the existing `spawn()` helper. Both open one repo.
2. Sidecar 1: `start_merge` of a conflicting branch → conflicted.
3. Sidecar 2: resolve (`resolve_whole_file` ours), then `commit`.
4. Both report `get_undo_state.undo == "Merge side"`.
5. Sidecar 1's `undo` → `Done`.

This passes with D3/D4 and locks in the cross-process path.

- [ ] Write → run. If it already passed before D3/D4 (the race isn't hit without the window),
  keep it as a smoke test and say so in the PR.
- [ ] Commit `vscode-sidecar: two sidecars share one undo history (#172)`.

---

## Part Z: docs, changelog, gate

### Task Z1: Docs and changelog

- [ ] `CHANGELOG.md` → `## [Unreleased]` → `### Rebase/undo safety`, replacing the stub:
  - `Fixed: starting a rebase over uncommitted changes could discard them (when the first step failed, and on Abort). A rebase now refuses to start while tracked files have staged or unstaged changes, as git does; untracked files are fine (#170).`
  - `While a cherry-pick/revert sequence is paused, deleting, renaming or adding a worktree on its branch and saving a stash are refused, as during a rebase (#171).`
  - `Fixed: with the desktop app and the VSCode extension open on one repository, finishing a paused operation in one could lose its undo entry, and two saves could drop an entry. Undo history changes now take a lock in .git/browsitory (#172).`
  - `Fixed: a merge, cherry-pick or revert aborted outside Browsitory and started again with the same commits reused the first one's undo snapshot. A paused merge or pick from the previous version is not undoable as one step after upgrading (#172).`
  - `With HEAD already holding a link and a folder whose names differ only in case, unrelated merges, cherry-picks, reverts, stash applies, pulls, rebases and switches to a branch with the same pair now work; only a new collision is refused (#167).`
  - `Rebase abort and cherry-pick/revert sequence abort are no longer refused when the tree they return to holds such a case collision (#161).`
- [ ] `docs/ARCHITECTURE.md` (spec "Docs"):
  - "Undo stack": the state lock, forced reloads, `LOCK_WAIT`, the marker mtime;
  - "Interactive rebase": the start refusal;
  - "Cherry-pick sequences": the gate scope;
  - "Worktree writes and symlinks": the HEAD and `also_known` exemptions, with the disk-scan
    argument (the line that today says such a repo "isn't locked" becomes accurate).
- [ ] `docs/USER_GUIDE.md`:
  - "Rebase": commit or stash first;
  - "Rewriting and moving commits": branch delete/rename/stash are refused while a sequence is
    paused.
- [ ] Commit `Docs and changelog for rebase/undo safety (#170, #171, #172, #161, #167)`.

### Task Z2: Full gate (CLAUDE.md order), then PR

Run everything from the repo root unless a `cd` is shown. Every step must be green before the
next.

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `cd frontend && pnpm install && pnpm lint && pnpm test -- --run && pnpm build && cd ..`
- [ ] `cd extension && pnpm install && pnpm test -- --run && pnpm run compile && cd ..`
- [ ] **Tauri E2E** (exact CLAUDE.md order; put `xvfb-run -a` in front of `pnpm test` when
  headless):
  ```bash
  cargo install tauri-driver --locked   # once, if missing
  cd frontend && VITE_E2E_REPO_PATH=/tmp/browsitory-e2e-repo pnpm build && cd ..
  cargo build --workspace --features tauri-app/custom-protocol,tauri-app/forge-fixture-override
  cd e2e && pnpm install && pnpm test && cd ..
  ```
  `rebase.spec.ts`, `restart-resilience.spec.ts`, `undo.spec.ts`,
  `cherry-pick-sequence.spec.ts` and `merge.spec.ts` must pass.
- [ ] **VSCode E2E** (exact CLAUDE.md order):
  ```bash
  cargo build --workspace
  cd frontend && pnpm run generate:release-notes && VITE_E2E_REPO_PATH=/tmp/browsitory-vscode-e2e-repo pnpm exec vite build --config vite.vscode.config.ts && cd ..
  cd extension && pnpm install && pnpm run compile && cd ..
  cd extension/e2e && pnpm install --ignore-workspace && pnpm test && cd ../..
  ```
- [ ] `python3 scripts/check-changelog.py --base origin/main` passes, and the `### Rebase/undo
  safety` entry is present.
- [ ] `grep -rn "\.tmp\"" crates/repo-service/src/worker` shows only `atomic_file.rs`.
- [ ] Manual smoke (`cargo tauri dev` from `crates/tauri-app`, on a scratch repo, never the real
  data dir):
  1. Edit a tracked file, "Rebase onto here", "Start rebase": the error names the file, and the
     edit is intact.
  2. Open the same scratch repo in the VSCode extension (dev host) and in the app. Start an
     Edit rebase in the app and Continue there while the VSCode view refreshes. Undo in either
     window shows "Rebase onto …".
  3. A paused cherry-pick sequence: renaming its branch from the sidebar is refused.
- [ ] Push. Open the PR with:
  - D1–D10 and their answers;
  - the A3 CI probe outcome;
  - "Closes #170, closes #171, closes #172, closes #161, closes #167";
  - the spec's unrelated findings, as follow-ups.

---

## Decision hooks

| Decision | If the user picks… | Change |
|---|---|---|
| D1-B | overlap only + path-limited restore | B1:<ul><li>`local_changes` always refuses staged changes;</li><li>unstaged changes are refused only within `plan_paths(repo, original_tree, onto, plan)` (HEAD→onto diff ∪ each non-Drop step's parent→commit diff);</li><li>`restore_original_branch` gains `paths`, recomputed at Abort as `plan_paths` ∪ diff(HEAD, index) ∪ conflicted paths, passed to `CheckoutBuilder::path` (+ `disable_pathspec_match`) and `prepare_forced_checkout_known(.., Some(paths), ..)`.</li></ul>New tests:<ul><li>`an_unrelated_unstaged_edit_survives_start_failure_and_abort` (`#[cfg(unix)]` for the gpg part);</li><li>`an_overlapping_unstaged_edit_is_refused`;</li><li>a perf note for 10k-path rebases.</li></ul>Copy: "Your changes to {path} … would be overwritten by the rebase." |
| D1-C | stash-and-rebase offer | A follow-up batch: `rebase.json` v2 `autostash: Option<oid>`, pop on Done/Abort, conflict handling, frontend confirm. B1 is unchanged (the refusal stays as the default). |
| D1-D | non-forced rollback/Abort | Drop B1's refusal. `restore_original_branch` uses a safe checkout; a refusal leaves the state stored with `stop = Error` and the error text. This needs new handling for a failed rollback in `start_rebase` (the state must be returned and stored). Not recommended. |
| D2-B/C | other copy | Change the `const`s and the E2E text. |
| D3-B | disable Start in the planner | A new frontend task: `RebasePlanner` gets `localChanges: boolean` from `status`, a `disabledReason` with the D2 text, and a Vitest test `start_is_disabled_with_uncommitted_changes`. The backend refusal stays. |
| D4-B | branch delete/rename only | C1: `refuse_stash_save = false` for sequences, and drop `stash_save_is_refused_while_a_sequence_is_paused`. Keep worktree-add (harmless). |
| D4-C | follow a rename | C1: no rename refusal. The worker's `RenameBranch` arm rewrites `sequence.json` `originalRef` when it matches. Test `renaming_the_sequence_branch_updates_the_stored_sequence` (abort still rewinds). |
| D5-B | `fs4` crate | D2 uses `fs4::fs_std::FileExt`, and a `docs/LICENSE_COMPLIANCE.md` row is added after `cargo info fs4` confirms MIT/Apache-2.0. |
| D5-C | O_EXCL lock file | D2 is rewritten: `create_new` + pid/timestamp content, a stale lock is one whose pid is gone or that is older than 10 min, and the killed-process test checks the takeover. Not recommended. |
| D6-B | wait indefinitely | `LOCK_WAIT = Duration::MAX` outside tests; drop `a_locked_command_runs_after_the_wait…`. |
| D6-C | refuse after wait | `gate::refuse_busy(command) -> Command` with an arm for every `locks_state` command and the D7 copy; the test becomes `a_locked_command_is_refused_when_the_lock_stays_held`. |
| D7-B | `UndoOutcome::Busy` | `UndoOutcome::Busy` is carried in the Tauri DTO, the sidecar dispatch, `RepoClient`/both clients, and the `useAppState` toast. Vitest and DTO tests are added. |
| D8-B | `created_at` comparison | D4: `PendingEntry.created_at: Option<u64>` (serde default); `settle` treats a pending entry as stale when the state file's mtime > `created_at`. The marker string stays unchanged, and so does the existing marker test. |
| D8-C | doc only | Drop the D4 marker change and its test; add an ARCHITECTURE note. |
| D9-B | stricter exemption | A1: `known` collisions are exempt only if, for every incoming source, the link's entry (mode+oid) equals HEAD's and no entry under the colliding folder differs from HEAD's. New test `a_merge_changing_d_x_under_a_known_collision_is_refused`. `switching_between_branches_holding_the_same_collision_is_allowed` flips to refused when `d/x` differs. |
| D9-C | fold-equal | A1: compare `fold_name(link), fold_name(folder)`. Drop `a_collision_spelled_differently_from_heads_is_not_known`. Not recommended. |
| D10-B | no E2E | Skip B3. |
