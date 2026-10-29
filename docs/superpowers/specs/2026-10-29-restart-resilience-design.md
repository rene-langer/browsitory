# Restart resilience: persisted rebase and pending undo

## Goal

A paused Browsitory operation and its undo entry survive an app restart, a VSCode window reload,
a closed tab and a worker crash:

- **#108:** the paused interactive rebase (`git_core::rebase::RebaseState`, today a worker-local
  `Option`) is persisted under `<gitdir>/browsitory/`, restored when a worker starts, gates other
  commands, and is reported in `HeadInfo`.
- **#134:** the pending undo entry of a paused operation (the snapshot taken before a
  cherry-pick/revert sequence, a rebase, a conflicted merge or a single pick) is persisted too,
  so an operation finished after a restart is still one undo step.

The batch also decides what happens when the stored state no longer matches the repository,
because the user ran git commands while Browsitory was closed.

## Shipping

One PR on `feat/restart-resilience`, one CHANGELOG section ("Restart resilience"). The work is
mostly backend: `git-core::rebase`, a new `repo-service` store, and the undo recorder. A thin
wire and frontend layer sits on top. The plan is
`docs/superpowers/plans/2026-10-29-restart-resilience.md`.

## Current state (main at 404b4ade, checked 2026-10-29)

### How Browsitory rebases (probe, libgit2 1.9 via git2 0.21)

Browsitory does **not** use libgit2's `git2::Rebase` API, and it does **not** write git's own
rebase state (`.git/rebase-merge/`, `.git/rebase-apply/`). `git_core::rebase` is its own state
machine:

- `start_rebase` checks out `onto` and detaches HEAD there. It never touches the branch ref until
  `finish`.
- Each step runs `repo.cherrypick(commit)` on the detached HEAD. Clean steps are committed
  (`land_current_step`). Squash/fixup groups are collapsed when the group ends.
- `finish` moves the original branch to the final commit, reattaches HEAD and calls
  `cleanup_state`.

A probe (`target/probe-restart`, not committed) showed what is on disk while a rebase is
paused:

| Pause | `repo.state()` | HEAD | Files in `.git` |
|---|---|---|---|
| Edit stop | `CherryPick` | detached | `CHERRY_PICK_HEAD`, `MERGE_MSG` |
| Conflict stop | `CherryPick` | detached | `CHERRY_PICK_HEAD`, `MERGE_MSG`, conflicted index |
| Error stop (step refused, e.g. a local edit in the way) | `Clean` | detached | none |

To the git CLI a paused Browsitory rebase is a **cherry-pick on a detached HEAD**. `git status`
says "You are currently cherry-picking commit …" and suggests `git cherry-pick
--continue/--abort`. `git rebase --continue` fails with "no rebase in progress". So:

- There is no `.git/rebase-merge` to reconcile with. The only on-disk git state that belongs to a
  paused Browsitory rebase is the detached HEAD, `CHERRY_PICK_HEAD`/`MERGE_MSG` and the index.
  Detecting a stale state means comparing the stored `expected_head`/`original_tip`/branch with
  HEAD, the branch ref, `repo.state()` and `CHERRY_PICK_HEAD`.
- The commands a user is likely to run outside the app are the ones `git status` suggests
  (`git cherry-pick --continue`, `--abort`, `git commit`), plus `git switch <branch>`, `git
  reset`, or deleting/moving the branch.
- If the user starts a real `git rebase` in a terminal, `repo.state()` is
  `RebaseInteractive`/`RebaseMerge`/`ApplyMailbox*`, which Browsitory treats as some other
  operation.

### Code

- `repo-service/src/worker/mod.rs`: `let mut rebase_state: Option<RebaseState> = None;` lives in
  the worker thread. `in_progress = repo.state() != Clean || rebase_state.is_some() ||
  sequence_store.load(repo).is_some()`. The comment says "Browsitory's own rebase leaves
  `repo.state()` clean", but that is only true for an error stop (see the table).
- `worker/rebase.rs`: `start` stores the state unless `Done`. `continue_rebase` keeps the
  mutated state on `Err`. `abort` `take()`s it before calling `git_core::rebase::abort_rebase`,
  so a failed abort loses the state. `progress` backs `GetRebaseProgress`.
- `git_core::rebase::RebaseState` has private fields `plan`, `cursor`, `original_branch_ref`,
  `original_tip` (`#[allow(dead_code)]`), `group_start_parent` and `expected_head`. It has no
  `Debug`/`Clone`/`PartialEq`.
- `rebase_continue` refuses `HeadMoved` when HEAD != `expected_head`, then always runs
  `land_current_step`, which commits whatever the index holds.
- `abort_rebase` always runs `restore_original_branch`: `set_head(branch)`, then a **forced**
  checkout, then `cleanup_state`.
- `worker/sequence_store.rs` (batch 4g-1) is the precedent: `<gitdir>/browsitory/sequence.json`,
  `version` 2 (reads 1..=2), temp file + `sync_all` + `rename`, and validation against the
  repository. Unreadable, malformed, unknown-version or foreign files are ignored and logged
  once per distinct reason (`last_warning`). It is loaded from disk on every use (no cache), so
  the Tauri app and the VSCode sidecar see each other's sequence. A stale but well-formed
  sequence stays visible and abortable. `continue_sequence` retries a step after an error stop
  or a terminal `--abort`, and accepts a conflicted step committed by hand when it is
  recognisable. `abort_sequence` only rewinds when HEAD is where the stop left it (`rewound:
  false` otherwise). `history::gate_during_sequence` refuses picks, merge, rebase,
  commit/amend, reset, checkout, switch, reflog restore, pull and stash apply/pop.
- `worker/undo.rs` `UndoRecorder`: `pending: Option<(String, RepoSnapshot)>` is in memory.
  - `finish` turns the active entry into `pending` while something is in progress.
  - `settle` drops `pending` when nothing is in progress any more.
  - `finish` records `pending` once nothing is in progress, unless the ending command was an
    abort.
  - `undo`/`redo` refuse while `pending` is set.
  - `undo_store.rs` persists only the two stacks (`undo.json`, `version` 1) and pins every
    snapshot object behind `refs/browsitory/undo/<main|wt/name>` plus keep refs. `StoredFile` has
    no `deny_unknown_fields`.
- Crash recovery: `ensure_worker` respawns a worker whose thread died (a panic in a command arm).
  Today that also loses the rebase.
- Frontend:
  - `useAppState.refresh` already calls `getRebaseProgress` and `getHeadInfo` on every refresh,
    so a restored backend state shows up after a restart with no frontend change.
  - `OperationStatusStrip`, `RebaseProgressPanel` and the palette key off `rebaseProgress`.
    During a rebase `mergeMessage` is also set (`MERGE_MSG`) and `headInfo.operation` is
    `CherryPick`. The rebase branch wins everywhere.
  - `App.tsx` keeps a backgrounded tab's close button disabled after unmount mid-rebase,
    because "rebase state lives only in the worker's in-memory `rebase_state`".
  - `startRebase`/`rebaseContinue` discard the `RebaseStepResult`, so a restart loses no
    frontend information.
- `extension/src` has no rebase or undo code. The sidecar and the frontend carry everything.

### Pre-existing bugs found while planning

1. **Continue after an error stop commits the step empty and loses its changes (data loss).**
   The repro was run with the probe: plan `[Edit a, Pick b]`. At the edit stop, make a local
   edit to a file `b` changes, then Continue. `a` lands, but `b`'s cherry-pick is refused ("1
   uncommitted change would be overwritten by merge"). `rebase_state` keeps cursor = b and the
   repository is `Clean`. Undo the local edit and Continue again: `land_current_step` commits the
   unchanged index as "b". The result is an **empty commit "b"**, with b's change silently
   dropped. The same happens after `git cherry-pick --abort` in a terminal during a pause. Fixed
   here (R2), since restart detection needs the same rule.
2. **Continue after a failed `finish` can never succeed.** With cursor past the end,
   `rebase_continue` returns `NotRebasing` and the only way out is Abort, which throws away every
   rebased commit. Fixed here: Continue retries `finish`.
3. **`finish` overwrites a branch that moved during the pause.** `original_tip` is recorded but
   never checked. While HEAD is detached the branch is not checked out anywhere, so another
   worktree (or `git branch -f`, or a pull in another clone) can move it. `finish` then
   `set_target`s over it and drops those commits from the branch (they remain only in the
   reflog). A restart makes the window longer. Fixed here (D4).
4. **A failed Abort loses the rebase state.** `worker/rebase.rs::abort` `take()`s the state
   before `abort_rebase` runs. If the forced checkout fails (for example a linked-folder refusal,
   #114), the user is stranded on a detached HEAD. Fixed here: the stored state is cleared only
   on `Ok`, as for sequences.
5. Misleading comment in `worker/mod.rs` ("Browsitory's own rebase leaves `repo.state()`
   clean"). It is replaced.

## Design

### 1. `git-core::rebase`: an inspectable, restart-safe state machine

`git-core` stays without serde. Its rebase API becomes as inspectable as `pick::SequenceState`,
so `repo-service` can store and rebuild it.

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebaseState {
    pub plan: Vec<RebasePlanEntry>,
    pub cursor: usize,                     // index of the stopped entry; == plan.len() after a failed finish
    pub original_branch_ref: String,       // "refs/heads/<name>"
    pub original_tip: Oid,                 // branch tip at start; compare-and-swap at finish (D4)
    pub onto: Oid,                         // new; for HeadInfo and the "Rebase onto" label
    pub group_start_parent: Option<Oid>,
    pub expected_head: Oid,
    pub stop: RebaseStop,                  // new
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebaseStop { Conflict, Edit, Error }
```

- `stop` records why the rebase is stopped. `Conflict` and `Edit` mean the current step was
  applied to the index (`CHERRY_PICK_HEAD` written). `Error` means a step or `finish` failed
  (`start_rebase` never returns an error state: it rolls back). The paths that return
  `Conflicted`/`PausedForEdit` set it, and every `Err` from `rebase_continue` sets `Error`.
- `RebaseState::progress() -> RebaseProgress { current_step, total_steps }` replaces the
  `current_step()`/`total_steps()` pair. Both methods are kept as thin wrappers so the existing
  tests stay unchanged.

**Health.** A new pure check, used by the worker for `HeadInfo` and by Continue/Abort:

```rust
pub enum RebaseHealth { Paused, Diverged(RebaseDivergence) }
pub enum RebaseDivergence { HeadMoved, BranchMoved, BranchMissing, OtherOperation }
pub fn rebase_health(repo: &Repository, state: &RebaseState) -> Result<RebaseHealth, RebaseError>;
```

The checks run in this order; the first that fails decides:

1. `original_branch_ref` doesn't resolve → `BranchMissing`.
2. The branch tip != `original_tip` → `BranchMoved`.
3. `repo.state()` is neither `Clean` nor `CherryPick`, or it is `CherryPick` but
   `CHERRY_PICK_HEAD` is not `plan[cursor]` → `OtherOperation`.
4. HEAD is not detached at `expected_head`:
   - If it is a step committed by hand that Continue would adopt (D2), → `Paused`.
   - Otherwise → `HeadMoved`.
5. Otherwise → `Paused`.

The cursor-past-end case (a failed `finish`) skips check 3's `CHERRY_PICK_HEAD` comparison.

**Continue** (`rebase_continue(repo, &mut state)`). The `match repo.state()` mirrors
`continue_sequence`:

| Situation | Action |
|---|---|
| `cursor == plan.len()` (finish failed earlier) | HEAD must equal `expected_head` (`HeadMoved` otherwise); retry `finish` (fixes bug 2) |
| `CherryPick`, `CHERRY_PICK_HEAD == plan[cursor]`, HEAD == `expected_head` | today's path: `land_current_step`, then `advance` |
| `CherryPick` with another commit, or any other non-clean state | `RebaseError::OperationInProgress` |
| `Clean`, HEAD detached == `expected_head` | **re-apply the step**: `advance` from `cursor` without landing (error stop, or `git cherry-pick --abort` outside; fixes bug 1) |
| `Clean`, `stop ∈ {Conflict, Edit}`, step committed by hand (D2) | adopt it: `expected_head = HEAD`; a `Reword` entry is re-committed with its new message; then the group bookkeeping that `land_current_step` does (collapse if this entry ends a group), `cursor += 1`, `advance` |
| anything else | `HeadMoved` (existing message) |

- Every `Err` sets `state.stop = Error`. The caller persists the state as returned (the cursor
  may have advanced past steps that landed).
- "Committed by hand" uses the sequence rule (`pick::is_step_committed_by_hand`). HEAD is
  detached and its only parent is `expected_head`. `CHERRY_PICK_HEAD` is gone. The author name,
  email and time equal the planned commit's, which `git commit`/`git cherry-pick --continue`
  keep during a cherry-pick. `Drop` entries are never current.

**Finish** checks the branch first (D4). If `original_branch_ref` is missing, it returns
`RebaseError::BranchMissing { branch }`. If the branch tip != `original_tip`, it returns
`RebaseError::BranchMoved { branch }`. Otherwise it moves the branch with
`Reference::set_target` and reflog "rebase (finish): returning to branch". An error leaves
`cursor == plan.len()`, `stop = Error`, and Continue retries `finish`.

**Abort** returns `RebaseAbort { restored: bool }` and takes `&RebaseState`. It mirrors
`abort_sequence`; the caller forgets the state only on `Ok`:

1. If git's paused pick is this rebase's step (`CherryPick` with `CHERRY_PICK_HEAD ==
   plan[cursor]`), or the state is `Clean` with HEAD at `expected_head` and a conflicted index:
   the rebase owns this state.
2. If HEAD is detached at `expected_head` and the branch exists, restore as today: `set_head` to
   the branch, prepare and run the forced checkout, then `cleanup_state`. `restored: true`. If
   the branch moved meanwhile, it is reattached at its **current** tip, which is right: abort
   means "back to the branch as it is".
3. Otherwise (HEAD moved, branch missing, or another operation is in progress), HEAD and refs
   stay where they are (D3). If case 1 applies, only this rebase's paused step is cleaned up with
   `merge::abort_merge`, which restores only the touched paths. `restored: false`.

New `RebaseError` variants and their exact copy:

- `OperationInProgress`: "Another merge, cherry-pick or revert is in progress. Finish or abort
  it before continuing the rebase."
- `BranchMoved { branch }`: "{short} moved since the rebase started. Abort the rebase and start
  again."
- `BranchMissing { branch }`: "{short} no longer exists. Abort the rebase."

`{short}` is the name without `refs/heads/`.

### 2. `repo-service`: `RebaseStore`

New `worker/rebase_store.rs`, a sibling of `sequence_store.rs` with the same shape and rules:

```rust
pub(super) struct RebaseStore { file: PathBuf, last_warning: RefCell<Option<String>> }
impl RebaseStore {
    pub fn open(repo: &Repository) -> Self;                       // <gitdir>/browsitory/rebase.json
    pub fn load(&self, repo: &Repository) -> Option<RebaseState>; // invalid → None, logged once
    pub fn save(&self, state: &RebaseState) -> Result<(), String>;// temp + sync_all + rename
    pub fn clear(&self) -> Result<(), String>;                    // missing file is fine
}
```

`rebase.json`, version 1:

```json
{
  "version": 1,
  "originalBranchRef": "refs/heads/main",
  "originalTip": "<oid>",
  "onto": "<oid>",
  "plan": [
    { "commitId": "<oid>", "action": { "kind": "Pick" }, "combinedMessage": null },
    { "commitId": "<oid>", "action": { "kind": "Reword", "message": "…" }, "combinedMessage": null }
  ],
  "cursor": 1,
  "groupStartParent": null,
  "expectedHead": "<oid>",
  "stop": "Conflict"
}
```

- **Versioning.** `FORMAT_VERSION = 1`, `OLDEST_READABLE_VERSION = 1`. No older file exists, so
  there is no migration. The module documents the rule `sequence_store` set: a later version that
  only adds fields with a safe default bumps `FORMAT_VERSION` and keeps reading the older one.
  An unknown version is ignored, like `sequence.json`, and the next `StartRebase` overwrites it.
- **Validation on load** (structural, against the repository; any failure is ignored and logged
  once):
  - the plan is non-empty and `cursor <= plan.len()`;
  - `originalBranchRef` starts with `refs/heads/` and is a valid reference name;
  - every `commitId`, `originalTip`, `onto`, `expectedHead` and `groupStartParent` parses as an
    oid and is a commit in this repository;
  - a `Reword` message is non-empty;
  - `plan[cursor]` is not `Drop` when `cursor < plan.len()`.

  Whether the state still matches HEAD and the branch is **not** a load concern. As with
  sequences, `rebase_health` reports it, so the user sees a stale rebase and can abort it.
- **Load on every use, no cache.** This follows `SequenceStore`. The Tauri app and the VSCode
  sidecar on the same repository share the state, and a crashed and respawned worker picks it up.
  The cost is a small JSON read plus one odb lookup per plan entry, several times per command
  (see Risks).
- **Writes.**
  - `StartRebase`: save when the result is `Conflicted`/`PausedForEdit`. Nothing is written on
    `Done` or `Err` (start rolls back).
  - `RebaseContinue`: clear on `Done`, otherwise save the returned state, `Err` included.
  - `AbortRebase`: clear only on `Ok` (fixes bug 4).

  A save that fails is reported with the outcome, like `history::persist`:
  - On a pause: "The rebase stopped, but its progress could not be saved ({error}). Abort it
    now, or finish it with git: resolve, `git cherry-pick --continue`, then `git switch
    {branch}`." The state is kept **in memory for this session** as a fallback (see below).
  - On `Err`: "{error} (its progress could not be saved: {store error})".
- **Session fallback.** The worker keeps `rebase_fallback: Option<RebaseState>`, used only when
  a save failed and cleared by the next successful save or clear. Without it, a full disk or a
  read-only `.git/browsitory` would strand the user mid-rebase in the same session, a regression
  from today. `load` order: the file, else the fallback.

**Gates.** `history::gate_during_sequence` becomes `worker::gate::gate_during_operation(command,
reason)`. The sequence and the stored rebase share it, with the same refused set: picks
(single and many), merge, rebase, commit/amend, reset, checkout, switch, reflog restore, pull,
and stash apply/pop. For both, the reply text stays git-core's `OperationInProgress` wording:
"Finish or abort the current merge/cherry-pick/revert/rebase first." Read-only commands, fetch,
staging and conflict resolution stay available. Today a paused rebase is only gated by the
frontend and by git-core's own state checks, which an error stop (`Clean`) bypasses.

**`in_progress`** becomes `repo.state() != Clean || rebase_store.load(repo).is_some() ||
sequence_store.load(repo).is_some()`, computed through one `OperationProbe` helper (§3).

**`HeadInfo.rebase: Option<RebaseInfo>`.** `git_core::repo::head_info` always sets `None`; the
worker's `GetHeadInfo` arm fills it, as it does `sequence`.

```rust
pub struct RebaseInfo {
    pub branch: String,          // short name
    pub onto: String,            // full oid
    pub current_step: usize,
    pub total_steps: usize,
    pub health: RebaseHealth,    // Paused | Diverged(reason)
}
```

`GetRebaseProgress` (worker command, `WorkerHandle::get_rebase_progress`, Tauri
`get_rebase_progress`, sidecar method, `RepoClient.getRebaseProgress`) is **removed**, and the
frontend reads `headInfo.rebase`. This mirrors the sequence decision in 4g-1 (one source, one
fewer call per refresh). The removal is its own task (F2), so it can be skipped without
affecting the rest.

### 3. Undo: the pending entry is persisted (#134 and its rebase/merge siblings)

**Where (D5): in `undo.json`**, not in `sequence.json`/`rebase.json` as #134 suggests. The
reasons:

- The pending entry is a full `RepoSnapshot` (HEAD, refs, index tree, worktree tree), not just
  "the HEAD before the sequence". `restore` needs all of it, and the index and worktree trees are
  loose, unreachable objects that `git gc` may prune. `undo_store` already pins snapshot objects
  behind its anchor commit; the per-operation files would need their own pinning.
- One mechanism covers every paused operation: sequence, rebase, conflicted merge, single
  conflicted cherry-pick/revert. Today a merge paused on a conflict and finished after a restart
  is also not recorded.
- The recorder already owns the pending lifecycle (`finish`/`settle`/`aborts`). Splitting it
  across stores would duplicate it.

**Format.** `undo.json` gains an optional field. The version stays **1** (D6):

```json
{ "version": 1, "undo": [...], "redo": [...],
  "pending": { "label": "Rebase onto 3f2a1bc", "before": { /* StoredSnapshot */ },
               "operation": "rebase refs/heads/main 9c0d…" } }
```

- `#[serde(default, skip_serializing_if = "Option::is_none")]`. An older build (no
  `deny_unknown_fields`) ignores the field and drops it on its next save, which only loses the
  pending entry: today's behaviour.
- `UndoStacks` gains `pending: Option<PendingEntry { label, before, operation }>`, so load, save
  and `reload_if_changed` cover it. `write_anchor` pins the pending `before` snapshot too, and
  `delete_stale`'s "everything empty" also requires `pending.is_none()`.

**Operation marker.** Before this change a pending entry was trusted because it was in memory.
Across a restart, or between two processes sharing `undo.json`, it must be tied to *the same*
operation. Otherwise a merge aborted in a terminal followed by a different merge would inherit
the first one's label and snapshot. A new
`OperationProbe::marker(repo, &rebase_store, &sequence_store) -> Option<String>` is checked in
this order:

1. A stored rebase: `rebase <originalBranchRef> <originalTip> <onto>`.
2. A stored sequence: `sequence <kind> <originalHead> <first commit> <commit count>`.
3. `repo.state()` `Merge`: `merge <HEAD> <MERGE_HEAD oids joined>`.
4. `CherryPick*`: `cherry-pick <HEAD> <CHERRY_PICK_HEAD>`.
5. `Revert*`: `revert <HEAD> <REVERT_HEAD>`.
6. Any other non-clean state: `git <state:?> <HEAD>`.
7. Otherwise `None`.

The rebase and sequence markers come first because their `CHERRY_PICK_HEAD` changes from step to
step. Every marker is constant for the whole life of its operation. `in_progress` is
`marker.is_some()`.

**Recorder changes:**

- `finish` stores the marker with the new pending entry and **saves**.
- `settle(marker)` first runs `reload_if_changed`, then drops a pending entry whose marker
  differs from the current one (`None` included) and saves. "Not in progress" is the `None`
  case, so in-session behaviour is unchanged.
- Recording or dropping a pending entry saves. `relabel` on a pending entry saves.
- `undo`/`redo` keep refusing while `pending` is set.

Concurrency: if the Tauri app and the VSCode sidecar both have the repository open, the one that
runs the ending command records the entry and the other sees it after `reload_if_changed`. This
is no worse than today's shared stacks.

**Does the rebase state hold an undo entry?** Not in `rebase.json`. The pre-rebase snapshot *is*
the pending undo entry ("Rebase onto X", scope Full), and it now persists through `undo.json`.
`RebaseState.original_tip` and `original_branch_ref` are enough for Abort, which is not undo.

### 4. Frontend (both hosts share it)

- `RepoClient.HeadInfo.rebase: RebaseInfo | null`, where `RebaseInfo = { branch, onto,
  currentStep, totalSteps, health: { kind: "Paused" } | { kind: "Diverged", reason: "HeadMoved"
  | "BranchMoved" | "BranchMissing" | "OtherOperation" } }`.
- `abortRebase(repoPath): Promise<RebaseAbort>`, where `RebaseAbort = { restored: boolean }`.
  `getRebaseProgress` is removed.
- `useAppState`: `state.rebaseProgress` is derived from `headInfo.rebase` (same shape, so the
  components and `commands.ts` keep their props), and `getRebaseProgress` leaves the refresh's
  `Promise.all`.
- **Restored state on start:** no new code path. A restarted worker loads `rebase.json`, and the
  first `refresh()` sees `headInfo.rebase` and `headInfo.sequence`. The strip, commit dock and
  palette show the paused rebase or sequence as before the restart. This holds for a Tauri
  restart, a closed and reopened tab, a respawned worker, and a VSCode window reload (new
  sidecar, restored webview, mount-time refresh).
- **Diverged rebase (D1).** `OperationStatusStrip` shows **"Rebase of {branch} interrupted"**
  with the reason, and **Abort rebase** only:
  - `HeadMoved`: "HEAD was moved outside Browsitory."
  - `BranchMoved`: "{branch} was changed outside Browsitory."
  - `BranchMissing`: "{branch} was deleted outside Browsitory."
  - `OtherOperation`: "Another git operation is in progress."

  The button's title says "Stops the rebase. HEAD stays where it is." `RebaseProgressPanel`
  disables Continue with the same reason as `disabledReason`. The palette omits "Continue
  rebase" while diverged and keeps "Abort rebase".
- **Abort toast** comes from the outcome (`lib/operationStatus.rebaseAbortToast`):
  - restored: "Aborted rebase";
  - not restored: "Stopped the rebase. HEAD had moved, so it was left where it is." (the
    sequence wording);
  - `BranchMissing`: "Stopped the rebase. {branch} no longer exists, so HEAD was left detached."
- **Tab close:** the `rebaseProgressRef` unmount exception in `App.tsx` is removed. A
  backgrounded tab mid-rebase is no longer kept busy forever, because closing it no longer loses
  the rebase. The active tab's close button stays disabled while an operation is in progress
  (`repositoryOperationDisabled`), as for merges.
- VSCode extension host (`extension/src`): no change. The webview CSP, sidecar lifecycle and
  `postMessage` bridge are untouched.

## Open decisions (provisional choice in **bold**; the plan implements it)

**D1. A stored rebase no longer matches the repository** (HEAD moved, branch moved or deleted,
or another operation started outside the app).

- A. Silently forget it on load (log only). The user sees a detached HEAD in "Cherry-picking",
  with no hint that a rebase was in progress.
- **B. Keep it, show "Rebase of {branch} interrupted" with the reason and Abort only. Abort never
  moves HEAD in that case (D3).** This matches sequences: a stale state stays visible and
  abortable and is never silently dropped. Cost: one click before starting the next operation.
- C. Like B, but forget automatically when HEAD is back on a branch and git has nothing in
  progress, which means the rebase was clearly abandoned (`git switch main`).
- D. Like B, plus "Resume anyway" (re-baseline `expected_head` to HEAD). Risky: it would land
  steps on top of unknown commits.

Recommendation **B**: it is consistent with 4g-1, loses nothing, and the friction is one click.
C is a reasonable later refinement once we see how often the abandoned case happens.

**D2. A paused step was committed outside the app** (`git cherry-pick --continue` or `git
commit`, which `git status` suggests).

- A. Refuse: `HeadMoved`, Abort only.
- **B. Adopt it when it is recognisable (as sequences do): the parent is `expected_head`,
  `CHERRY_PICK_HEAD` is gone, the author matches, and the stop was `Conflict`/`Edit`. A `Reword`
  step gets its planned message.**
- C. Adopt any single commit on top of `expected_head`.

Recommendation **B**: it is parity with sequences, and following git's own hint shouldn't cost
the user their rebase.

**D3. Abort when the rebase diverged.**

- **A. Clean up only this rebase's paused step, leave HEAD and refs, and forget the state
  (`restored: false`).** This is the `abort_sequence` rule.
- B. Always force back to the branch, which is today's behaviour. It throws away commits made on
  the detached HEAD (reflog only) and uncommitted edits.
- C. Ask: a confirm dialog offering "Return to {branch} (discard changes)" or "Leave HEAD".

Recommendation **A**: never destroy work the app didn't create. Going back to the branch is one
"Switch" away once the state is forgotten.

**D4. The original branch moved during the pause.**

- **A. `finish` refuses with `BranchMoved`, the strip shows it as diverged, and the user
  aborts.**
- B. Keep today's overwrite.
- C. Rebase the new commits too, which needs a new plan and is out of scope.

Recommendation **A**: B silently drops commits from a branch.

**D5. Where the pending undo entry is stored** (deviates from #134's suggestion).

- **A. `undo.json` `pending`, pinned by the undo anchor, generic for every paused operation.**
- B. Inside `sequence.json`/`rebase.json`, which needs separate gc pinning and still leaves
  merges/single picks unrecorded after a restart.
- C. A separate `undo-pending.json`. This needs two-file ordering with the anchor.

Recommendation **A**.

**D6. `undo.json` version for the new field.**

- **A. Stay at 1. The field is optional and older builds ignore it.**
- B. Bump to 2. An older build then sees an "unknown version", blocks writes and loses all undo
  history after a downgrade.

Recommendation **A**. A version bump is kept for changes an older build would misread.

**D7. E2E.**

- **A. One Tauri spec that restarts the app with WebdriverIO `browser.reloadSession()` (deletes
  the tauri-driver session, which closes the app, then launches a fresh one against the same
  `XDG_DATA_HOME`), time-boxed to a spike. If tauri-driver/WebKitWebDriver doesn't relaunch
  cleanly, drop it and rely on the Rust restart tests plus the sidecar restart round-trip.**
- B. No E2E: Rust restart tests (worker drop and respawn, crash respawn), the sidecar
  process-restart round-trip, and Vitest only.

  A VSCode E2E restart is not feasible: `workbench.action.reloadWindow` kills the
  `@vscode/test-electron` runner, which lives in the extension host. The sidecar restart
  round-trip (`protocol_roundtrip.rs`, a real sidecar process killed and started again) covers
  that path below the UI.

## Testing

- **git-core `tests/rebase.rs`** (real temp repos):
  - the state's public fields, `onto` and `stop` per pause kind;
  - Continue after an error stop re-applies the step (the bug 1 repro, asserting that b's change
    lands and no empty commit exists);
  - Continue after a terminal `git cherry-pick --abort` re-applies the step (done with libgit2:
    `cleanup_state` plus a forced `checkout_head`, which is what `--abort` does);
  - Continue refuses another paused cherry-pick and a merge (`OperationInProgress`);
  - adopting a step committed by hand: conflict stop and edit stop; `Reword` gets its message; a
    squash group member adopted mid-group still collapses; refused for an error stop or an
    unrecognisable commit;
  - `finish` refuses `BranchMoved`/`BranchMissing` and a later Continue retries `finish` after
    the branch is put back;
  - `rebase_health` for each divergence;
  - Abort: restores when HEAD is at `expected_head`; does not move HEAD after it moved; leaves
    HEAD detached when the branch is gone; reattaches to a moved branch's new tip; keeps the
    state on a failed abort (worker level).
- **repo-service `rebase_store`** (unit, like `sequence_store`):
  - round trip;
  - a new store on the same repository sees the state;
  - corrupt, empty, binary, unknown-version and `kind`-less files are ignored and logged once;
  - foreign ids, an out-of-range cursor, a `Drop` at the cursor, an empty `Reword` and a non-head
    ref are ignored;
  - a directory in place of the file makes save and clear fail;
  - the wire format is camelCase with `version: 1`.
- **repo-service worker** (restart = drop the `Worker` and every handle, then `Worker::spawn`
  the same path):
  - a paused rebase survives a restart (`get_head_info().rebase` is `Some`, Paused, step 1 of 2);
  - continue after a restart finishes it;
  - abort after a restart restores the branch;
  - a worker crash (`crash_for_test` + `ensure_worker`) keeps the rebase;
  - an error stop is persisted with its cursor;
  - the gates refuse switch, pick, commit, merge and start-rebase while a rebase is stored, and
    staging and conflict resolution stay allowed;
  - a diverged rebase is reported in `HeadInfo` after HEAD moved outside;
  - a failed save keeps the session fallback (save into a directory in place of the file).
- **Undo** (worker):
  - a sequence finished after a restart is one undo entry (#134);
  - so is a rebase finished after a restart, and a merge;
  - an operation aborted outside while closed drops the pending entry;
  - a different operation started outside doesn't inherit it;
  - a rebase aborted after a restart records nothing;
  - undo refuses while a restored pending entry exists.

  `undo_store`: pending round trip; a file without `pending` loads; pending objects survive `git
  gc --prune=now`; empty stacks plus a pending entry keep the anchor; an older reader ignores
  `pending` (deserialize with a struct lacking the field).
- **tauri-app DTOs:** `HeadInfoDto.rebase` (Paused and Diverged), `RebaseAbortDto`.
- **vscode-sidecar `protocol_roundtrip.rs`:** `a_paused_rebase_survives_a_sidecar_restart`
  starts a rebase that stops for an edit, kills the sidecar process, starts a new one, checks
  `get_head_info.rebase`, then continues to `Done`. It also updates
  `rebase_lifecycle_round_trips_through_the_sidecar` (abort result, no `get_rebase_progress`).
- **Frontend** (mock `RepoClient`):
  - both clients' wiring (`abortRebase` result, `headInfo.rebase`, no `getRebaseProgress`);
  - `useAppState` derives `rebaseProgress` from `headInfo.rebase` on the first refresh;
  - the strip's paused and diverged labels for each reason;
  - the panel disables Continue with the reason;
  - the palette omits Continue while diverged;
  - abort toasts by outcome;
  - a backgrounded tab mid-rebase is not kept busy.
- **E2E (D7-A):** `e2e/specs/restart-resilience.spec.ts` starts an Edit rebase, calls
  `browser.reloadSession()`, waits for "Rebasing, step 1 of 2", clicks Continue, and checks the
  log, that `rebase.json` is gone, and that Undo is enabled with "Rebase onto …".

## Docs

- `docs/ARCHITECTURE.md`:
  - a new "Interactive rebase" section: the state machine, what a pause looks like to git, the
    `rebase.json` format and validation, health, Continue/Abort rules, gates;
  - "Undo stack": the pending entry is persisted, with its marker;
  - "Cherry-pick sequences": delete the sentence "The pending undo entry is in memory, …".
- `docs/USER_GUIDE.md` "Rebase": a paused rebase survives restarts; what "interrupted" means;
  that `git status` calls a paused Browsitory rebase a cherry-pick, and that Continue accepts a
  step finished with `git cherry-pick --continue`.
- `CHANGELOG.md` `## [Unreleased]`, `### Restart resilience` (wording in the plan's Task Z1).
- No `docs/LICENSE_COMPLIANCE.md` change: no new dependency.

## Risks

- **Load cost per command.** Each command loads `rebase.json` and `sequence.json` several times
  (settle, begin, gate, marker, finish), with one odb lookup per plan entry. For a 200-entry
  plan that is about 1,000 lookups per command, which is cheap for libgit2 but measurable. The
  mitigation, if it shows up in profiling: cache the parsed file keyed by `(mtime, len)`. Not
  done up front, matching `SequenceStore`.
- **Two processes on one repository** (Tauri plus VSCode). Both now see the rebase and can both
  press Continue. The second Continue fails with `HeadMoved`/`OperationInProgress` and doesn't
  corrupt anything. `undo.json` writes are last-writer-wins, as today.
- **Detection is heuristic.** Adoption (D2) relies on author identity and time. A user who
  amends the author while committing by hand gets `HeadMoved`, then Abort (safe).
- **The behaviour of in-session Abort changes** (D3). Users who relied on Abort forcing them back
  after they had moved HEAD now get "left where it is". This is documented in the CHANGELOG.
- **Removing `get_rebase_progress`** touches about 12 files across both transports. It is
  isolated in one task (F2).
- **E2E restart** (D7) is unproven with tauri-driver. It is time-boxed and has a fallback.
- **Forced checkout on Abort** (unchanged from today, parity with `git rebase --abort`) still
  discards uncommitted edits made during the pause when HEAD is where the rebase left it.

## Out of scope

- Writing git-compatible `.git/rebase-merge` state, so the CLI would see a rebase instead of a
  cherry-pick.
- Marking the branch as being rebased so other worktrees can't check it out.
- A cache for the stores.
- Auto-forgetting abandoned rebases (D1-C).
- A VSCode E2E restart.
- An "edit this step's message" option on Continue.
