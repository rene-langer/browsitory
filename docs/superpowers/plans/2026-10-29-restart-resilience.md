# Restart Resilience Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close #108 (a paused Browsitory rebase is lost on restart) and #134 (the pending undo
entry of a paused operation is lost on restart). The rebase state goes to
`<gitdir>/browsitory/rebase.json`. The pending undo entry goes to `undo.json`, tied to its
operation by a marker. Stale state is reported as "interrupted", not silently dropped. Three
pre-existing rebase bugs found while planning are fixed on the way.

**Architecture:**
- `git-core::rebase` becomes an inspectable state machine (public `RebaseState` fields, a `stop`
  kind, `rebase_health`), with a safe Continue/Abort decision table modelled on
  `pick::continue_sequence`/`abort_sequence`.
- `repo-service` gains `worker/rebase_store.rs` (a sibling of `sequence_store.rs`), a shared
  operation gate, an `OperationProbe` (in-progress plus marker), and a persisted pending undo
  entry in `undo_store.rs`.
- `HeadInfo.rebase` replaces `get_rebase_progress` on the wire. The frontend derives
  `rebaseProgress` from it and adds a "Rebase of X interrupted" state.

**Tech Stack:** Rust (`git2` 0.21 / libgit2 1.9, `serde`/`serde_json` in `repo-service`),
React + Vitest, WebdriverIO + tauri-driver.

**Spec:** `docs/superpowers/specs/2026-10-29-restart-resilience-design.md`. This plan implements
the **provisional** choice of each decision D1–D7. "Decision hooks" at the end lists what changes
for the other options.

---

## Global Constraints

- **Branch:** `feat/restart-resilience` (already pushed with the spec and plan). One PR, opened
  only when Task Z2 passes.
- **No new dependency.** No `docs/LICENSE_COMPLIANCE.md` change.
- **Tests use real temp-dir repos** (`tempfile` + `git2::Repository::init`), never a mocked
  `Repository`. The frontend mocks `RepoClient`, never `@tauri-apps/api` or `postMessage`.
- **Hermetic tests:** a new integration test binary starts with `test_support::hermetic_git!()`
  (or `mod common;`). Unit tests in `src/` are covered by each crate's `#[cfg(test)]
  hermetic_git!()`. Repo helpers pin `commit.gpgSign=false` and never rely on
  `init.defaultBranch`: read the branch from `HEAD`.
- **Restart in tests** means: drop the `Worker` **and every `WorkerHandle`**, then
  `Worker::spawn(path)` again. Held replies guarantee the old worker persisted everything before
  its reply was delivered. Add the helper once, in `worker/mod.rs` tests:
  `fn restart(worker: Worker, handle: WorkerHandle, path: &Path) -> (Worker, WorkerHandle)`.
- **No CLI git in Rust tests unless it is unavoidable.** Simulate terminal commands with libgit2.
  - `git cherry-pick --abort` = `cleanup_state` + `reset(HEAD, Hard)`.
  - `git commit` during a pick = `index.write_tree` + `repo.commit(Some("HEAD"), &picked.author(),
    …)` + `cleanup_state`.
  - `git switch` = `set_head` + forced `checkout_head`.
- **Never** touch `~/.local/share/com.browsitory.app` or the real config dir. E2E uses its
  `XDG_DATA_HOME` sandbox. Probes go under `target/`.
- **Error copy is exact** (spec §1, §4). Put it in `const`s and assert it verbatim once.
- **CHANGELOG:** every push touching `crates/`, `frontend/src/` or `e2e/` needs a `CHANGELOG.md`
  change (pre-push hook `scripts/check-changelog.py`). Task R1 adds a stub line under
  `## [Unreleased]` → `### Restart resilience`, and Task Z1 writes the final text.
- **Commits:** one per task unless noted, message `<area>: <what> (#108|#134)`. Each ends with
  the session's `Co-Authored-By`/`Claude-Session` trailers.

## Pre-flight

- [ ] `git status` is clean on `feat/restart-resilience`, rebased on `origin/main` if `main`
  moved. Re-check the line references below if it did.
- [ ] `cargo test -p git-core --test rebase && cargo test -p repo-service` passes before any
  change. This is the baseline.
- [ ] Link the pre-push hook (CLAUDE.md "Git hooks") if this worktree doesn't have it.

Reconciled with the code at 404b4ade on 2026-10-29:

- `crates/git-core/src/rebase.rs`:
  - `RebaseState` has private fields; `original_tip` is `#[allow(dead_code)]`;
  - `rebase_continue` (l. 230) always calls `land_current_step` after the `HeadMoved` check;
  - `advance` (l. 263) leaves the cursor on a step whose `cherrypick` failed;
  - `finish` (l. 463) never checks `original_tip`;
  - `abort_rebase` (l. 475) always forces.
- `crates/repo-service/src/worker/mod.rs`:
  - `rebase_state` is declared at l. 741 and the `in_progress` closure at l. 749;
  - the sequence gate is applied at l. 773;
  - the rebase arms are at l. 1281–1290 and the undo arms at l. 1462–1473.
- `worker/rebase.rs::abort` `take()`s the state before `abort_rebase` (bug 4).
- `worker/history.rs`: `gate_during_sequence`, `persist`, `head_info`.
- `worker/undo.rs` `UndoRecorder { store, stacks, active, pending }`; `worker/undo_store.rs`
  `StoredFile { version, undo, redo }`, `write_anchor`, `delete_stale`.
- Wire: `get_rebase_progress` exists in:
  - `crates/tauri-app/src/commands/rebase.rs` and `main.rs`;
  - `crates/vscode-sidecar/src/dispatch.rs` (`"get_rebase_progress"`);
  - the frontend: `RepoClient.ts`, `tauriRepoClient.ts`, `vscodeRepoClient.ts`, `useAppState.ts`.
  - It is also in these tests and mocks: `vscodeRepoClient.test.ts`, `useAppState.test.ts`,
    `App.test.tsx`, `RepoPicker.test.tsx`, `DiffPane.test.tsx`, `RebasePlanner.test.tsx`,
    `ConflictResolutionPane.test.tsx`.
- `HeadInfo` literals: `crates/git-core/src/repo.rs` (2), `crates/tauri-app/src/commands/mod.rs`
  tests, `crates/vscode-sidecar/src/dispatch.rs` (`HeadInfoDto`, l. 2542). In the frontend,
  `sequence: null` appears in `useAppState.ts`, `DiffPane.tsx`, and these tests: `App.test.tsx`,
  `RepoPicker.test.tsx`, `CommitGraph.test.tsx`, `DiffPane.test.tsx`, `commands.test.ts`. Each
  gains `rebase: null`.
- `App.tsx` l. 486–504: the `rebaseProgressRef` unmount exception (removed in F4).
- The probe confirming that a paused rebase is a `CherryPick` state with no `.git/rebase-merge`,
  and the bug 1 repro, are in `target/probe-restart` (not committed; see spec "Current state").

## Pre-flight notes

Reconciled again by the implementer on 2026-10-29 (main had not moved from 404b4ade; baseline
`cargo test -p git-core --test rebase` 27 passed, `cargo test -p repo-service` 177 passed). The
line references above held. Where the plan or spec and the code disagreed, the code won; every
deviation made during implementation:

- **R1/R2, `stop` on errors.** The spec says every `Err` from `rebase_continue` sets `Error`, but
  R2's own test requires a refused Continue to leave the state untouched. Refusals (`HeadMoved`,
  `OperationInProgress`, `NotRebasing`) now leave the state as it was; any later error sets
  `Error` unless the step is still applied and paused (git's pick is still `plan[cursor]`). This
  also keeps D2 adoption available after a refused Continue.
- **R2, `land_current_step` clears git's pick.** After landing a step it now calls
  `cleanup_state`. Otherwise an error between two steps (e.g. the linked-folder check before the
  next pick) or a failed `finish` would leave the previous step's `CHERRY_PICK_HEAD`, and
  Continue would refuse with `OperationInProgress`. Error stops and failed finishes are now
  always `Clean`.
- **R2, "HEAD at `expected_head`" means detached there.** The spec requires it for the clean
  row; it now also applies to the paused-pick row, so a branch created at `expected_head` during
  the pause (`git switch -c`) is never moved by a landed step. `HeadMoved` names the branch.
- **R4, `cursor == plan.len()`.** Instead of skipping the `CHERRY_PICK_HEAD` comparison, the
  retry requires git to be clean (`OperationInProgress` otherwise, since `finish` runs
  `cleanup_state` and would wipe a foreign pick); `rebase_health` reports `OtherOperation`
  likewise. A Continue on a rebase that already finished (HEAD on the branch at the result)
  still reports `NotRebasing`, keeping the existing test. `finish` uses `reference_matching`
  (atomic compare-and-swap); a lost race maps to `BranchMoved`.
- **R6.** `RebaseError::Merge` was added for `merge::abort_merge`. `tests/symlink_write_through.rs`
  also called `abort_rebase` by value (not listed above) and was updated.
- **S2, bug 4.** Keeping the state on a failed Abort was not enough: `restore_original_branch`
  reattached HEAD before the forced checkout, so a refused checkout left HEAD on the branch with
  a half-restored work tree. It now checks out the branch's tree first and reattaches after.
  The failed-Abort test uses a read-only folder that the checkout must create a file in
  (`#[cfg(unix)]`, skipped when running as root).
- **S2, session fallback order.** The spec loads the file first, then the fallback. After a
  failed save the file can hold an *older* state of the same rebase, so the fallback (set only
  by a failed save, cleared by the next successful save or clear) wins while it is set
  (`gate::stored_rebase`).
- **S2 tests.** A rebase replayed onto its own base within the same second can reproduce the old
  ids, so `a_restarted_worker_continues_the_rebase_to_completion` checks the result, not that the
  tip changed. For the same reason the rebase undo test (U2) stages an amendment at the edit
  stop, so the rebase changes something to record.
- **S3/U2.** `OperationProbe` has no `in_progress`: the worker passes `marker()` and "in
  progress" is `marker.is_some()`. The merge marker reads `MERGE_HEAD` directly
  (`mergehead_foreach` needs `&mut Repository`). `settle` takes the repository so a dropped
  pending entry is saved. `relabel` does not save by itself: it only runs when a sequence
  completes, and the `finish` right after it records (and saves) the entry.
- **W2.** `rebase_lifecycle_round_trips_through_the_sidecar` never pauses, so it now checks
  `get_head_info.rebase == null` instead of `get_rebase_progress`, and the abort result is covered
  by a new `abort_rebase_reports_whether_head_went_back_to_the_branch`.
- **F1.** `HeadInfo` literals also lived in `useMutationRunner.test.ts` and `CommitGraph.test.tsx`
  (not all listed above).
- **F3.** `RebaseProgressPanel` already had `disabled`/`disabledReason`, so the "disables Continue
  with the divergence reason" test lives in `DiffPane.test.tsx` only. The palette gets App's
  toasting `abortRebase` through `buildCommands({ ...appState, abortRebase }, …)`. The
  interrupted strip's Abort is disabled while an action is pending.
- **Z1.** The CHANGELOG also has a line for bug 4 (a failed Abort no longer strands HEAD).
- **E1.** The spike succeeded on the first run: `browser.reloadSession()` deletes the
  tauri-driver session (closing the app) and starts a new one (a fresh app process on the same
  `XDG_DATA_HOME`); two distinct session ids were confirmed in the log. The spec is kept. Local
  E2E runs used `XDG_CONFIG_HOME`/`XDG_CACHE_HOME`/`XDG_DATA_HOME`/`XDG_STATE_HOME` under
  `target/e2e-xdg/`.
- The rebase undo label is the pre-existing `Rebase onto <full id>` (the planner passes the full
  id); the E2E matches its prefix.

## File map

| File | Tasks | Change |
|---|---|---|
| `crates/git-core/src/rebase.rs` | R1–R6 | public state, `stop`, `onto`, health, Continue table, finish CAS, `RebaseAbort` |
| `crates/git-core/tests/rebase.rs` | R1–R6 | new tests |
| `crates/git-core/src/repo.rs` | S4 | `HeadInfo.rebase`, `RebaseInfo` |
| `crates/git-core/tests/head_info.rs` | S4 | `rebase` is `None` from git-core |
| `crates/repo-service/src/worker/rebase_store.rs` (new) | S1 | store and unit tests |
| `crates/repo-service/src/worker/gate.rs` (new) | S3 | `gate_during_operation`, `OperationProbe` |
| `crates/repo-service/src/worker/rebase.rs` | S2, S4, W1 | persist via store, fallback, health |
| `crates/repo-service/src/worker/history.rs` | S3 | gate moved out, `head_info` fills `rebase` |
| `crates/repo-service/src/worker/mod.rs` | S2–S4, U2 | store wiring, probe, restart tests |
| `crates/repo-service/src/worker/undo_store.rs` | U1 | `pending` in file and anchor |
| `crates/repo-service/src/worker/undo.rs` | U2 | marker, persisted pending |
| `crates/tauri-app/src/commands/{mod,rebase}.rs`, `main.rs` | W1, F2 | DTOs, abort result, remove progress |
| `crates/vscode-sidecar/src/dispatch.rs`, `tests/protocol_roundtrip.rs` | W2, F2 | same; restart round-trip |
| `frontend/src/ipc/{RepoClient,tauriRepoClient,vscodeRepoClient}.ts` (+tests) | F1, F2 | types, abort result |
| `frontend/src/state/{useAppState,useMergeRebaseActions}.ts` (+tests) | F2, F3 | derive progress, abort outcome |
| `frontend/src/lib/{operationStatus,commands}.ts` (+tests) | F3 | labels, toasts, palette |
| `frontend/src/components/{OperationStatusStrip,RebaseProgressPanel,DiffPane}.tsx` (+tests) | F3 | diverged UI |
| `frontend/src/App.tsx`, `App.test.tsx` | F3, F4 | toast, unmount exception removed |
| `e2e/specs/restart-resilience.spec.ts` (new) | E1 | restart flow (spike) |
| `docs/ARCHITECTURE.md`, `docs/USER_GUIDE.md`, `CHANGELOG.md` | Z1 | docs |

Parallelism: Part R is sequential. S1 depends only on R1. U1 is independent of R and S.
U2 needs S3 (`OperationProbe`). W/F need S4. E1 needs F. Two lanes are safe: {R → S → W → F →
E} and {U1 → (after S3) U2}.

---

## Part R: `git-core::rebase`

### Task R1: Inspectable `RebaseState` with `onto` and `stop`

**Goal:** Make `RebaseState` public (fields `pub`, `#[derive(Debug, Clone, PartialEq, Eq)]`), add
`onto: Oid` and `stop: RebaseStop { Conflict, Edit, Error }`, and add
`progress() -> RebaseProgress { current_step, total_steps }`. Keep `current_step()` and
`total_steps()`. Remove `#[allow(dead_code)]` from `original_tip`. Behaviour is unchanged.

**Files:** `crates/git-core/src/rebase.rs`, `crates/git-core/tests/rebase.rs`, `CHANGELOG.md`
(stub).

**TDD requirement** (`tests/rebase.rs`):
- `a_conflict_pause_records_stop_conflict_onto_and_the_original_tip`: `stop == Conflict`, `onto`
  == the onto commit, `original_tip` == the pre-rebase branch tip, `original_branch_ref` ==
  `refs/heads/<branch from HEAD>`, `expected_head` == HEAD.
- `an_edit_pause_records_stop_edit`.
- `progress_reports_the_paused_step_one_based` (step 1 of 2 at the first pause).

- [ ] Step 1: write the tests; they fail to compile (private fields, no `stop`).
- [ ] Step 2: implement. `advance` sets `stop` before returning `Conflicted`/`PausedForEdit`.
  `start_rebase` and `rebase_continue` set `stop = Error` on every `Err` path that keeps the
  state (a wrapper: `fn mark_error<T>(state: &mut RebaseState, r: Result<T, RebaseError>) -> …`).
- [ ] Step 3: `cargo test -p git-core --test rebase` passes. Add `### Restart resilience` with
  the stub line "- A paused rebase survives an app restart (in progress)." under `##
  [Unreleased]`.
- [ ] Commit `git-core: inspectable rebase state with onto and stop kind (#108)`.

### Task R2: Continue re-applies a step that isn't applied (bug 1)

**Goal:** With `repo.state() == Clean` and HEAD detached at `expected_head`, `rebase_continue`
re-runs `advance` from `cursor` instead of `land_current_step`. A cherry-pick state whose
`CHERRY_PICK_HEAD` is not `plan[cursor]`, and any other non-clean state, → new
`RebaseError::OperationInProgress` (spec copy).

**TDD requirement:**
- `continue_after_an_error_stop_retries_the_step_instead_of_committing_it_empty`. This is the
  probe repro: plan `[Edit a, Pick b]` (b changes `f`). At the edit pause, write a local change
  to `f`. Continue → `Err`, `stop == Error`, cursor on b, state `Clean`. Restore `f` and Continue
  → `Done`. HEAD's commit "b" has `f == "from b\n"` and a non-empty diff to its parent.
  **Fails on main.**
- `continue_after_the_paused_pick_was_aborted_outside_reapplies_the_step`: at a conflict pause,
  simulate `git cherry-pick --abort` (`cleanup_state` + hard reset). Continue → `Conflicted`
  again on the same step.
- `continue_refuses_while_another_cherry_pick_is_paused`: at an edit pause, `cleanup_state` and
  then `repo.cherrypick(unrelated)` → `OperationInProgress`; the state is untouched.
- `continue_refuses_during_a_merge` (write `MERGE_HEAD`) → `OperationInProgress`.

- [ ] Tests → the first fails. Implement the decision table rows 2–4 of the spec §1 table.
  Re-read the index (`repo.index()?.read(true)`) before using it, so a terminal `git add` is
  seen (sequence precedent).
- [ ] Commit `git-core: rebase Continue re-applies a step that never landed instead of committing it empty (#108)`.

### Task R3: Adopt a paused step committed by hand (D2-B)

**Goal:** `Clean`, `stop ∈ {Conflict, Edit}`, HEAD detached with exactly one parent ==
`expected_head`, `CHERRY_PICK_HEAD` absent, and the author (name, email, time) equal to
`plan[cursor]`'s → adopt the step:
- set `expected_head = HEAD`;
- for a `Reword` entry, replace HEAD with a commit of the same tree, parent and author but the
  planned message (`set_head_detached`);
- run the group bookkeeping that `land_current_step` does after its commit: factor that into
  `after_landed(repo, state)`, which collapses if `ends_a_group`;
- `cursor += 1`, then `advance`.

Share the author/parent check with `pick` by extracting `pick::is_step_committed_by_hand`'s core
into `pub(crate) fn commit_matches_pick(repo, head, expected_parent, picked) -> bool` in
`pick.rs`, and use it from both.

**TDD requirement:**
- `continue_adopts_a_conflicted_step_committed_with_git`: resolve the conflict, then simulate
  `git commit` (keeping the picked author). Continue → `Done`, the branch has the step once, and
  no duplicate commit exists.
- `continue_adopts_an_edit_step_committed_with_git`.
- `continue_gives_an_adopted_reword_step_its_planned_message`.
- `continue_adopts_a_squash_member_and_still_collapses_the_group`: plan `[Pick a, Squash b
  (conflicts)]`. Commit b by hand, Continue → one commit with the combined message.
- `continue_does_not_adopt_after_an_error_stop` → `HeadMoved`.
- `continue_does_not_adopt_a_commit_with_another_author` → `HeadMoved`.
- `pick_sequence` tests stay green (shared helper).

- [ ] Tests → fail. Implement.
- [ ] Commit `git-core: rebase Continue adopts a paused step committed with git (#108)`.

### Task R4: `finish` compare-and-swap and retry (D4-A, bug 2)

**Goal:** `finish` refuses with `BranchMissing { branch }` / `BranchMoved { branch }` (spec copy)
before moving anything. `rebase_continue` with `cursor == plan.len()` checks HEAD ==
`expected_head` (else `HeadMoved`) and retries `finish`.

**TDD requirement:**
- `finish_refuses_when_the_branch_moved_during_the_pause`: at an edit pause, move the branch
  (`reference(refs/heads/<b>, other, force)`). Continue → `BranchMoved`, the branch still points
  at `other`, HEAD stays detached, and `cursor == plan.len()`.
- `continue_retries_finish_after_the_branch_is_put_back` → `Done`, with the branch at the
  rebased tip.
- `finish_refuses_when_the_branch_was_deleted` → `BranchMissing`.
- `the_branch_errors_name_the_short_branch` (exact copy).

- [ ] Tests → fail. Implement.
- [ ] Commit `git-core: rebase finish refuses a branch moved during the pause and Continue retries it (#108)`.

### Task R5: `rebase_health`

**Goal:** `pub fn rebase_health(repo, &RebaseState) -> Result<RebaseHealth, RebaseError>` with
`RebaseHealth::{Paused, Diverged(RebaseDivergence::{HeadMoved, BranchMoved, BranchMissing,
OtherOperation})}`, in the spec's check order. A step committed by hand that R3 would adopt
counts as `Paused`: share R3's predicate.

**TDD requirement:**
- `health_is_paused_at_a_conflict_an_edit_and_an_error_stop`.
- `health_is_head_moved_after_switching_to_a_branch_outside`.
- `health_is_head_moved_after_a_commit_that_is_not_the_step`.
- `health_is_paused_when_the_step_was_committed_by_hand`.
- `health_is_branch_moved_and_branch_missing`.
- `health_is_other_operation_for_a_foreign_cherry_pick_and_a_merge`.

- [ ] Tests → fail. Implement.
- [ ] Commit `git-core: rebase_health classifies a stored rebase against the repository (#108)`.

### Task R6: `abort_rebase` → `RebaseAbort { restored }` (D3-A)

**Goal:** `pub fn abort_rebase(repo, &RebaseState) -> Result<RebaseAbort, RebaseError>`, per the
spec §1 "Abort" rules. `start_rebase`'s rollback keeps calling `restore_original_branch`
directly.

**TDD requirement:**
- `abort_restores_the_branch_when_head_is_where_the_rebase_left_it` (conflict pause; the
  existing behaviour, `restored: true`).
- `abort_after_an_error_stop_restores_the_branch`.
- `abort_after_head_moved_cleans_up_only_its_own_step_and_leaves_head`: switch to another branch
  outside while `CHERRY_PICK_HEAD` stays. Abort → `restored: false`, HEAD still on the other
  branch, state `Clean`, and an unrelated uncommitted edit survives.
- `abort_with_the_branch_deleted_leaves_head_detached` → `restored: false`, HEAD detached at
  `expected_head`, state clean.
- `abort_with_the_branch_moved_reattaches_to_its_new_tip` → `restored: true`, HEAD on the
  branch at the moved tip.
- `abort_leaves_a_foreign_operation_alone`: start a merge outside → `restored: false`, and
  `MERGE_HEAD` still exists.

- [ ] Tests → fail. Implement. Update the existing callers and tests that passed the state by
  value (`abort_rebase(&repo, state)` → `&state`).
- [ ] Commit `git-core: rebase abort never moves HEAD the rebase no longer owns (#108)`.

---

## Part S: persistence in `repo-service`

### Task S1: `RebaseStore`

**Goal:** `worker/rebase_store.rs`, per spec §2: `open`/`load`/`save`/`clear`, `StoredRebase`
v1 (camelCase, `action` tagged `{"kind": …, "message"?}`), `FORMAT_VERSION = 1`,
`OLDEST_READABLE_VERSION = 1`, validation, and a log once per distinct reason. Mirror
`sequence_store.rs` line for line where possible. Do not share code by generics; the two files
stay independently readable.

**Depends on:** R1.

**TDD requirement** (`#[cfg(test)] mod tests` in the new file):
- `saves_and_loads_a_state_round_trip` (no temp file left; clearing twice is fine);
- `a_new_store_on_the_same_repo_sees_the_saved_state`;
- `a_corrupt_or_unknown_state_file_is_ignored`: `{not json`, empty, binary, `{"version":1}`,
  `version: 2`, an action `{"kind":"Rebase"}`;
- `a_state_that_does_not_fit_this_repository_is_ignored`: a foreign oid in the plan, a bad
  `expectedHead`, `cursor > plan.len()`, an empty plan, `Drop` at the cursor, an empty `Reword`
  message, `originalBranchRef` = `HEAD` / `refs/tags/x`;
- `a_cursor_past_the_end_after_a_failed_finish_loads`;
- `a_directory_in_place_of_the_file_is_ignored_and_reported_on_save`;
- `the_wire_format_is_camel_case_with_a_version`;
- `a_linked_worktree_has_its_own_rebase_file` (`repo.path()` of a linked worktree).

- [ ] Tests → fail. Implement, and add `mod rebase_store;` to `worker/mod.rs`.
- [ ] Commit `repo-service: RebaseStore persists a paused rebase to rebase.json (#108)`.

### Task S2: The worker keeps the rebase in the store

**Goal:** Replace `rebase_state: Option<RebaseState>` with `rebase_store: RebaseStore` plus
`rebase_fallback: Option<RebaseState>` (spec §2 "Session fallback").
`worker/rebase.rs::{start, continue_rebase, abort}` follow the spec §2 "Writes" rules through a
`persist(store, fallback, state_after, outcome)` helper modelled on `history::persist`. Abort
clears only on `Ok` (bug 4). Fix the misleading comment. `GetRebaseProgress` reads the store; it
is removed in F2.

**Depends on:** S1, R6.

**TDD requirement** (`worker/mod.rs` tests; add the `restart` helper):
- `a_paused_rebase_survives_a_worker_restart`: Edit pause, restart, then
  `get_rebase_progress() == Some((1, 2))`.
- `a_restarted_worker_continues_the_rebase_to_completion`: the branch moves, HEAD is attached,
  and `rebase.json` is gone.
- `a_restarted_worker_aborts_the_rebase`: the branch is untouched, HEAD is attached, and the
  file is gone.
- `a_crashed_worker_keeps_the_paused_rebase`: `crash_for_test`, then `ensure_worker` on the same
  map, then the progress is reported.
- `a_rebase_error_stop_is_persisted_with_its_cursor` (the R2 scenario through the worker plus a
  restart, then Continue lands b with content).
- `a_failed_abort_keeps_the_stored_rebase`: make the forced checkout fail (on unix, replace a
  tracked folder with a symlink to a second `TempDir`, so `prepare_forced_checkout` removes the
  link; if that path now succeeds, use a read-only file inside the work tree instead and
  `#[cfg(unix)]`). If no reliable failure exists, test it at the `persist` helper level with an
  `Err` outcome.
- `a_failed_save_keeps_the_rebase_for_the_session`: create a directory at
  `browsitory/rebase.json` before the start. Start → `Err` text containing "could not be saved"
  and the branch name. `get_rebase_progress()` is still `Some`, and Continue works in-session.
- The existing `rebase_continue_and_abort_rebase_round_trip_through_the_worker` stays green.

- [ ] Tests → fail. Implement.
- [ ] Commit `repo-service: persist the paused rebase and restore it when a worker starts (#108)`.

### Task S3: One gate and one probe for every stored operation

**Goal:** Move `history::gate_during_sequence` to `worker/gate.rs` as
`gate_during_operation(command) -> Command`, with the same refused set and text. Add
`OperationProbe<'a> { rebase: &'a RebaseStore, fallback: &'a Option<RebaseState>, sequence: &'a
SequenceStore }` with `in_progress(repo) -> bool` and `marker(repo) -> Option<String>` (spec §3
marker table). The worker loop builds the probe once per command and gates when
`rebase.load().is_some() || sequence.load().is_some()`.

**Depends on:** S2.

**TDD requirement:**
- `commands_are_refused_while_a_rebase_is_stored`: during an Edit pause, `switch_branch`,
  `cherry_pick`, `cherry_pick_many`, `commit`, `amend_commit`, `start_merge`, `start_rebase`,
  `reset_to_commit`, `checkout_commit` and `pop_stash` each reply with the
  `OperationInProgress` text, and HEAD is unchanged.
- `commands_are_refused_after_a_rebase_error_stop` (state `Clean`; this is where the gate is
  new).
- `staging_and_conflict_resolution_stay_available_during_a_rebase`.
- `marker_is_stable_for_the_whole_operation`: a unit test in `gate.rs` checking the rebase marker
  across two steps, the sequence marker across two steps, and the merge marker.
- `marker_prefers_the_stored_rebase_over_its_cherry_pick_state`.
- The existing sequence gate tests pass unchanged (only the import moves).

- [ ] Tests → fail. Implement.
- [ ] Commit `repo-service: one operation gate and probe for stored rebases and sequences (#108)`.

### Task S4: `HeadInfo.rebase`

**Goal:** `git_core::repo::RebaseInfo { branch, onto, current_step, total_steps, health:
RebaseHealth }`, plus `HeadInfo.rebase: Option<RebaseInfo>` (always `None` from git-core). The
worker's `head_info` fills it from the probe's stored rebase and `rebase_health`. A health error
counts as `Diverged(OtherOperation)` and is logged.

**Depends on:** S3, R5.

**TDD requirement:**
- `tests/head_info.rs`: `head_info_never_reports_a_rebase_itself`.
- Worker: `get_head_info_reports_a_paused_rebase_after_restart` (branch short name, onto, 1 of
  2, Paused).
- Worker: `get_head_info_reports_a_diverged_rebase_when_head_moved_outside`: restart, simulate
  `git switch <branch>` with libgit2, then `HeadMoved`, while the gate still applies.
- Worker: `get_head_info_reports_no_rebase_after_it_finishes`.
- Update every `HeadInfo { … }` literal (`rebase: None`).

- [ ] Tests → fail. Implement.
- [ ] Commit `repo-service: report the stored rebase and its health in HeadInfo (#108)`.

---

## Part U: persisted pending undo entry (#134)

### Task U1: `undo.json` carries `pending`

**Goal:** `UndoStacks.pending: Option<PendingEntry { label, before: RepoSnapshot, operation:
String }>`, `StoredFile.pending` (`#[serde(default, skip_serializing_if = "Option::is_none")]`),
version still 1 (D6-A). `write_anchor` includes `pending.before`; `delete_stale`'s `empty` also
needs `pending.is_none()`.

**Depends on:** none (parallel with R/S).

**TDD requirement** (`undo_store.rs` tests):
- `saves_and_reloads_the_pending_entry`.
- `a_file_without_pending_loads_with_none`: the existing v1 JSON literal.
- `an_older_reader_ignores_the_pending_field`: deserialize the saved JSON into a test-local copy
  of the old `StoredFile` (no `pending`), which succeeds.
- `pending_alone_keeps_the_anchor`: empty stacks plus pending → the anchor ref exists; clearing
  pending → the anchor is deleted.
- `gc_keeps_the_pending_snapshot_objects` (git CLI, skipped like the existing gc tests): a
  `Full` snapshot with a staged change and an untracked file survives `reflog expire` + `gc
  --prune=now`.

- [ ] Tests → fail. Implement.
- [ ] Commit `repo-service: undo.json keeps the pending entry of a paused operation (#134)`.

### Task U2: The recorder persists `pending` with its operation marker

**Goal:** `UndoRecorder` uses `stacks.pending` instead of its own field:
- `finish` stores `PendingEntry { …, operation: marker }` and saves;
- `settle(marker: Option<&str>)` runs `reload_if_changed`, drops a pending entry whose marker
  differs (`None` included), and saves;
- recording or dropping a pending entry saves, and so does `relabel` on a pending entry.

The worker passes `probe.marker(&repo)` to `settle`/`begin`/`finish`/`undo`/`redo` in place of
`in_progress(...)` (`in_progress == marker.is_some()`).

**Depends on:** U1, S3.

**TDD requirement** (`worker/mod.rs` tests):
- `a_sequence_finished_after_a_restart_is_one_undo_entry` (#134's exact case): cherry-pick 3
  with a conflict in the middle, restart, resolve, `continue_sequence` → `undo_state().undo ==
  Some("Cherry-pick 3 commits")`. Undo restores the original HEAD.
- `a_rebase_finished_after_a_restart_is_one_undo_entry` ("Rebase onto …"; undo restores the
  original branch tip).
- `a_merge_finished_after_a_restart_is_one_undo_entry`: conflicted merge, restart, resolve,
  commit → "Merge <branch>".
- `a_rebase_aborted_after_a_restart_records_nothing`.
- `an_operation_aborted_outside_while_closed_drops_the_pending_entry`: conflicted merge, drop the
  worker, `cleanup_state` + hard reset with libgit2, spawn → `undo_state` unchanged, and
  `undo.json` has no `pending`.
- `a_different_operation_started_outside_does_not_inherit_the_pending_entry`: conflicted merge of
  A, drop, abort, merge B (conflict) with libgit2, spawn, resolve, commit → nothing recorded with
  "Merge A". Recording "Merge B" is impossible (its `before` is unknown): assert nothing new.
- `undo_refuses_while_a_restored_pending_entry_exists` → `OperationInProgress`.
- The existing `stage_during_a_conflicted_merge_is_not_recorded_and_abort_drops_pending`,
  `a_conflicted_sequence_finished_by_continue_is_one_undo_entry` and the held-reply guards stay
  green.

- [ ] Tests → fail. Implement.
- [ ] Commit `repo-service: a paused operation's undo entry survives a restart (#134)`.

---

## Part W: wire

### Task W1: Tauri DTOs

**Goal:** `HeadInfoDto.rebase: Option<RebaseInfoDto>` (`{ branch, onto, currentStep,
totalSteps, health: { kind: "Paused" } | { kind: "Diverged", reason } }`). The `abort_rebase`
command returns `RebaseAbortDto { restored }`. `WorkerHandle::abort_rebase` returns
`Result<RebaseAbort, String>`.

**Depends on:** S4.

**TDD requirement** (`crates/tauri-app/src/commands/mod.rs` tests):
- `head_info_dto_serializes_a_paused_rebase`.
- `head_info_dto_serializes_a_diverged_rebase_reason`.
- `rebase_abort_dto_serializes_restored`.

- [ ] Tests → fail. Implement. `cargo build -p tauri-app`.
- [ ] Commit `tauri-app: HeadInfo.rebase and the abort-rebase result (#108)`.

### Task W2: Sidecar, including a real process restart

**Goal:** The same DTOs in `dispatch.rs` (`HeadInfoDto`, `abort_rebase` result).

**Depends on:** W1.

**TDD requirement** (`crates/vscode-sidecar/tests/protocol_roundtrip.rs`):
- `a_paused_rebase_survives_a_sidecar_restart`: the first `Sidecar::spawn()` runs
  `open_repo`/`start_rebase` (Edit, Pick) → `PausedForEdit`. `drop` it (kill + wait), then a
  second `Sidecar::spawn()` runs `open_repo` and `get_head_info` → `rebase.currentStep == 1`,
  `health.kind == "Paused"`, then `rebase_continue` → `Done`, then `get_head_info.rebase ==
  null`.
- `a_sequence_finished_after_a_sidecar_restart_is_one_undo_entry` (#134 through the VSCode
  path): conflict in a 2-commit cherry-pick, restart, resolve with `resolve_whole_file`,
  `continue_sequence`, then `get_undo_state.undo == "Cherry-pick 2 commits"`.
- Update `rebase_lifecycle_round_trips_through_the_sidecar` (abort returns `{ restored: true }`).

- [ ] Tests → fail. Implement.
- [ ] Commit `vscode-sidecar: HeadInfo.rebase, abort result, restart round-trips (#108, #134)`.

---

## Part F: frontend

Run in `frontend/`: `pnpm test -- --run <paths>`, then `pnpm test -- --run && pnpm lint` at the
end of each task.

### Task F1: Types and both clients

**Goal:** `RepoClient.ts`: `RebaseInfo`, `RebaseDivergence`, `HeadInfo.rebase`, `RebaseAbort`,
`abortRebase(): Promise<RebaseAbort>`. Map them in `tauriRepoClient.ts` and
`vscodeRepoClient.ts`. Add `rebase: null` to every `HeadInfo` fixture listed in Pre-flight.

**TDD requirement:** `tauriRepoClient.test.ts` / `vscodeRepoClient.test.ts`:
- `abortRebase resolves to the restored flag`;
- `getHeadInfo passes rebase through, including a diverged reason`.

- [ ] Tests → fail. Implement.
- [ ] Commit `frontend: RebaseInfo in HeadInfo and the abort-rebase result (#108)`.

### Task F2: `rebaseProgress` from `headInfo.rebase`; remove `getRebaseProgress`

**Goal:**
- `useAppState.refresh` drops `client.getRebaseProgress` and sets `rebaseProgress =
  headInfo.rebase ? { currentStep, totalSteps } : null`.
- Remove `getRebaseProgress` from `RepoClient`, both clients and every mock.
- Remove the Tauri command (`commands/rebase.rs`, `main.rs`), the sidecar method,
  `WorkerHandle::get_rebase_progress`, `Command::GetRebaseProgress` and `worker/rebase.rs::progress`.
- Rust tests that used `get_rebase_progress()` switch to `get_head_info().rebase`.

**Depends on:** F1, W2.

**TDD requirement:** `useAppState.test.ts`:
- `derives rebase progress from headInfo.rebase on the first refresh`;
- `clears rebase progress when headInfo.rebase is null`.

Then `grep -rn "getRebaseProgress\|get_rebase_progress\|GetRebaseProgress" crates frontend/src
extension/src` is empty.

- [ ] Tests → fail. Implement. `cargo test --workspace` and the frontend suite pass.
- [ ] Commit `Read rebase progress from HeadInfo; remove get_rebase_progress (#108)`.

### Task F3: Interrupted rebase UI and abort toast (D1-B, D3-A)

**Goal:**
- `lib/operationStatus.ts`: `rebaseDivergenceText(reason, branch)` and `rebaseAbortToast(abort,
  info)` (spec §4 copy).
- `OperationStatusStrip` gets a `rebase: RebaseInfo | null` prop. When diverged it shows "Rebase
  of {branch} interrupted", the reason, and an "Abort rebase" button whose title is "Stops the
  rebase. HEAD stays where it is.". When paused, the label is unchanged.
- `RebaseProgressPanel`, through `DiffPane`: Continue is disabled with `disabledReason` = the
  divergence text.
- `lib/commands.ts`: omit `rebase-continue` while diverged.
- `useMergeRebaseActions.abortRebase` resolves to `RebaseAbort | null`. `App.tsx` toasts
  `rebaseAbortToast`, replacing the generic announce for abort-rebase (strip, dock and palette
  all go through one `abortRebase` callback, like `abortSequence`).

**TDD requirement:**
- `operationStatus.test.ts`: `rebaseDivergenceText covers each reason`; `rebaseAbortToast for
  restored, moved, and branch missing`.
- `OperationStatusStrip.test.tsx`: `shows the interrupted label and reason for a diverged
  rebase`; `keeps "Rebasing, step N of M" while paused`.
- `RebaseProgressPanel.test.tsx` / `DiffPane.test.tsx`: `disables Continue with the divergence
  reason`.
- `commands.test.ts`: `omits Continue rebase while the rebase is interrupted but keeps Abort`.
- `App.test.tsx`: `toasts "Stopped the rebase. HEAD had moved…" when abort did not restore`; `a
  paused rebase reported on the first refresh shows the strip and the dock` (the restart case:
  the mock `getHeadInfo` returns `rebase` from the start).

- [ ] Tests → fail. Implement.
- [ ] Commit `frontend: show an interrupted rebase and toast the abort outcome (#108)`.

### Task F4: A background tab mid-rebase is no longer stuck busy

**Goal:** Remove `rebaseProgressRef` and its effect in `App.tsx`. The unmount cleanup always
reports `false`, as for merges. Rewrite the comment: the state is on disk and is re-read on
remount.

**TDD requirement:** `App.test.tsx`: `switching away from a tab mid-rebase does not keep its
close button disabled`. Mount two repos with repo A rebasing, switch to B, and A's close button
is enabled. This replaces any test that asserted the old exception; find it with `grep -n
"mid-rebase\|rebaseProgressRef" frontend/src/App.test.tsx`.

- [ ] Test → fail. Implement.
- [ ] Commit `frontend: closing a tab no longer loses a paused rebase, so it isn't kept busy (#108)`.

---

## Part E: E2E

### Task E1 (spike, time-boxed to 2 hours): Tauri restart flow (D7-A)

**Goal:** `e2e/specs/restart-resilience.spec.ts`:
1. Commits on a side branch (the `rebase.spec.ts` helpers).
2. Through the UI: "Rebase onto here" with the first row set to **Edit**, then "Start rebase".
3. The strip shows "Rebasing, step 1 of 2".
4. `await browser.reloadSession()` (the app process exits and a fresh one starts against the
   same `XDG_DATA_HOME`). Wait for the E2E repo to load.
5. The strip shows "Rebasing, step 1 of 2" again.
6. Click "Continue rebase".
7. `git log -2 --format=%s` shows both commits on the branch, `.git/browsitory/rebase.json` is
   gone, and the Undo button's title or label contains "Rebase onto".

- [ ] Step 1: write the spec and run it alone: `pnpm test -- --spec ./specs/restart-resilience.spec.ts` (full build sequence from CLAUDE.md, under `xvfb-run` if headless).
- [ ] Step 2: if `reloadSession` doesn't relaunch the app (tauri-driver keeps the old process,
  or the new session can't find the window) within the time box, **delete the spec**. Record
  "D7 fell back to B: <reason>" in the PR body. The Rust/sidecar restart tests cover the path.
- [ ] Commit `e2e: a paused rebase survives an app restart (#108)` (only if kept).

No VSCode E2E: `workbench.action.reloadWindow` kills the `@vscode/test-electron` runner (spec
D7). `a_paused_rebase_survives_a_sidecar_restart` covers the sidecar path.

---

## Part Z: docs, changelog, gate

### Task Z1: Docs and changelog

- [ ] `CHANGELOG.md` → `## [Unreleased]` → `### Restart resilience`, replacing the R1 stub:
  - `A paused rebase (conflict, edit stop or error) now survives an app restart, a VSCode window reload, closing its tab and a backend crash, and the status strip shows it again on start (#108).`
  - `A cherry-pick/revert sequence, rebase or conflicted merge finished after a restart is still recorded as one undo step (#134).`
  - `If the repository was changed outside Browsitory while a rebase was paused (HEAD moved, the branch moved or was deleted, another git operation started), the strip shows "Rebase of <branch> interrupted" with the reason. Abort then stops the rebase without moving HEAD or discarding work it no longer owns.`
  - `Continue accepts a paused rebase step that was finished with git (git cherry-pick --continue or git commit).`
  - `Fixed: continuing a rebase after a step failed (for example because a local edit was in the way) committed that step empty and lost its changes.`
  - `Fixed: a rebase whose branch moved during the pause no longer overwrites the branch; Continue reports it, and a failed final step can be retried instead of only aborted.`
- [ ] `docs/ARCHITECTURE.md`:
  - new section "Interactive rebase" after "Cherry-pick sequences": what a pause looks like to
    git (the spec's table), `rebase.json` format, validation and versioning rule, health and
    Continue/Abort tables, gates, the session fallback;
  - "Undo stack": the persisted pending entry and its marker;
  - "Cherry-pick sequences": delete "The pending undo entry is in memory, so a sequence finished
    after an app restart is not undoable as one step."
- [ ] `docs/USER_GUIDE.md` "Rebase": the rebase survives a restart; what "interrupted" means and
  what Abort does then; that `git status` calls a paused Browsitory rebase a cherry-pick, and
  that finishing a step with `git cherry-pick --continue` is fine.
- [ ] Commit `Docs and changelog for restart resilience (#108, #134)`.

### Task Z2: Full gate (CLAUDE.md order), then PR

Run everything from the repo root unless a `cd` is shown. Every step must be green before the
next.

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `cd frontend && pnpm install && pnpm lint && pnpm test -- --run && pnpm build && cd ..`
- [ ] `cd extension && pnpm install && pnpm test -- --run && pnpm run compile && cd ..`
- [ ] **Tauri E2E** (exact CLAUDE.md order; `xvfb-run -a` in front of `pnpm test` when headless):
  ```bash
  cargo install tauri-driver --locked   # once, if missing
  cd frontend && VITE_E2E_REPO_PATH=/tmp/browsitory-e2e-repo pnpm build && cd ..
  cargo build --workspace --features tauri-app/custom-protocol,tauri-app/forge-fixture-override
  cd e2e && pnpm install && pnpm test && cd ..
  ```
  `rebase.spec.ts`, `undo.spec.ts`, `cherry-pick-sequence.spec.ts` and (if kept)
  `restart-resilience.spec.ts` must pass.
- [ ] **VSCode E2E** (exact CLAUDE.md order):
  ```bash
  cargo build --workspace
  cd frontend && pnpm run generate:release-notes && VITE_E2E_REPO_PATH=/tmp/browsitory-vscode-e2e-repo pnpm exec vite build --config vite.vscode.config.ts && cd ..
  cd extension && pnpm install && pnpm run compile && cd ..
  cd extension/e2e && pnpm install --ignore-workspace && pnpm test && cd ../..
  ```
- [ ] `python3 scripts/check-changelog.py --base origin/main` passes (the pre-push hook runs it
  with `--base main`). The `### Restart resilience` entry is present.
- [ ] `grep -rn "getRebaseProgress\|get_rebase_progress\|rebase_state" crates frontend/src extension/src` is empty, apart from deliberate mentions in comments, if any.
- [ ] Manual smoke (`cargo tauri dev` from `crates/tauri-app`, on a scratch repo, never the
  real data dir):
  1. Start an Edit rebase, quit the app, start it again: the strip and dock are back. Continue
     works and Undo shows "Rebase onto …".
  2. Repeat, but run `git switch <branch>` in a terminal while the app is closed: "Rebase of
     <branch> interrupted / HEAD was moved outside Browsitory". Abort → toast "Stopped the
     rebase…", HEAD stays put.
  3. A cherry-pick sequence with a conflict, quit, reopen, resolve, Continue: Undo shows
     "Cherry-pick N commits".
- [ ] Push. Open the PR with: the decisions D1–D7 and their answers; the E1 outcome; "Closes
  #108, closes #134"; the pre-existing bugs fixed (spec bugs 1–4).

---

## Decision hooks

| Decision | If the user picks… | Change |
|---|---|---|
| D1-A | silently forget | S4: the worker `clear()`s a stored rebase whose health is `Diverged` (log once) and reports `rebase: None`. Drop F3's diverged UI and its tests; D3 then only applies in-session. |
| D1-C | auto-forget when HEAD is back on a branch with nothing in progress | S4: forget only for `Diverged(HeadMoved)` with HEAD attached and state `Clean`; the rest as B. New worker test `an_abandoned_rebase_is_forgotten_when_head_is_back_on_a_branch`. |
| D1-D | "Resume anyway" | New `RebaseContinue { rebaseline: bool }`, a git-core `rebaseline(state)` setting `expected_head = HEAD`, and a strip button; tests for landing on top of moved HEAD. Not recommended. |
| D2-A | refuse steps committed by hand | Drop R3; R5 has no adoption branch; remove the CHANGELOG line about `git cherry-pick --continue`. |
| D2-C | adopt any single commit | R3 drops the author check (keeps parent == `expected_head`, `CHERRY_PICK_HEAD` absent). Flip `continue_does_not_adopt_a_commit_with_another_author`. |
| D3-B | always force back | R6 keeps today's `restore_original_branch` for every case except `BranchMissing` (which can't reattach); `restored` is always true otherwise. Drop the "left where it is" toast. |
| D3-C | confirm dialog | F3 adds a `ConfirmDialog` when diverged ("Return to {branch} and discard changes" / "Leave HEAD"); `abort_rebase` gains `force: bool` through the wire (W1/W2/F1). |
| D4-B | overwrite a moved branch | Drop R4's CAS part; keep the retry part. |
| D5-B | per-operation files | U1/U2 are replaced by `pendingUndo` fields in `sequence.json` (v3) and `rebase.json`, plus a keep-ref pin per file; merges and single picks stay unrecorded after a restart. |
| D5-C | separate `undo-pending.json` | U1 writes a second file after the anchor; the U1 tests move to the new file. |
| D6-B | bump `undo.json` to 2 | U1: `FORMAT_VERSION = 2`, read 1..=2; CHANGELOG note that downgrading loses undo history. |
| D7-B | no E2E | Skip E1. |
