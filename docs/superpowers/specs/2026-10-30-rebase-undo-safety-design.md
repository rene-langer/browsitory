# Rebase/undo safety

## Goal

Close five follow-ups from the restart-resilience (#169) and security follow-up (#160) batches:

| Issue | Area | Problem |
|---|---|---|
| #170 | data loss | Starting a rebase doesn't refuse local changes. Its rollback after a failed start, and Abort, use a forced checkout that discards them. |
| #171 | gate | A paused cherry-pick/revert sequence doesn't protect its branch: the branch can be renamed or deleted. |
| #172 | concurrency | The undo store races when two processes (Tauri app plus VSCode sidecar) or two workers share a repository. A restarted identical merge also reuses a stale pending entry. |
| #161 | case collisions | Rebase abort and sequence abort are refused with `CaseCollision` when the tree they restore holds a link/folder case collision. |
| #167 | case collisions | With HEAD already holding such a collision, unrelated merges, cherry-picks, stash applies and other operations are refused. |

## Shipping

One PR on `fix/rebase-undo-safety`, with one CHANGELOG section ("Rebase/undo safety") under
`## [Unreleased]`. The changes are backend only (`git-core`, `repo-service`), plus one Tauri
E2E case. There is no wire, DTO or frontend change unless D3-B is chosen. The plan is
`docs/superpowers/plans/2026-10-30-rebase-undo-safety.md`.

## Current state (main at 6044d7bc, checked 2026-10-30)

### #170: a probe of what a rebase does with local changes

A throwaway binary under `target/` (deleted afterwards) linked against `git-core` and
`repo-service` and ran in hermetic temp repositories. `feature` is `base(f.txt, shared.txt) →
g(g.txt)`. `onto` is `base → h(h.txt)`. The local edit is to `f.txt`, a file that **neither the
onto diff nor any step touches**.

| # | Setup | Result |
|---|---|---|
| 1 | Unstaged edit to `f.txt`, `commit.gpgSign=true`, `gpg.program=/bin/false`, plan `[Pick g]` | Start fails ("Signing failed…"). The rollback puts HEAD back on `feature`, and **`f.txt` is reset to `f0`: the edit is lost.** |
| 2 | Unstaged edit to `f.txt`, plan `[Edit g]`, then Abort | The edit survives the start and the edit stop. **Abort (`restored: true`) resets it.** |
| 2b | Staged edit to `f.txt`, plan `[Edit g]` | libgit2's cherry-pick refuses **every** step while the index differs from HEAD ("1 uncommitted change would be overwritten by merge"). The rollback then **discards the staged change** (`f.txt` = `f0`, status clean). |
| 3 | Untracked `h.txt` (identical to, or different from, onto's) | The safe checkout of onto refuses ("1 conflict prevents checkout") before HEAD moves. Nothing is lost. |
| 4 | Case 1 through a `Worker` | Start fails and `f.txt` is reset. The undo stack then holds **"Rebase onto \<full oid\>"**, and Undo restores `f.txt` = `DIRTY`. |

What this shows:

- The loss is wider than #170 says. `restore_original_branch` forces a checkout of the
  **whole** tree, so the rollback and Abort discard every tracked local change, whether or not
  it overlaps the plan. Refusing only on overlapping changes is not enough unless the restore
  is also limited to the plan's paths (D1-B).
- A staged change of any path makes the first pick fail. A rebase can therefore never succeed
  with a staged change, and the failure destroys that change.
- **Undo does recover it today (case 4).** The worker's `begin` captures a Full snapshot (the
  worktree tree includes modified files), the failed command's `finish` records "before →
  after", and Undo restores `before`. The user has to know to press Undo after an error,
  though, and the entry's label carries a 40-character id (see "Unrelated findings").
- Abort's forced checkout of edits made **during** the pause is the same as `git rebase --abort`
  and stays as it is. It is listed as an accepted risk in the restart-resilience spec.

### #171: what a paused sequence protects

`worker::gate::gate_during_operation(command, rebased_branch)` refuses picks, merge, rebase,
commit/amend, reset, checkout, branch switch/create, reflog restore, pull and stash apply/pop
while a rebase **or** a sequence is stored. Branch delete/rename, worktree add on the branch,
and stash save are refused only when `rebased_branch` is `Some`, which is only for a rebase.

During a paused sequence:

- **Rename:** libgit2 renames the checked-out branch and repoints HEAD. `abort_sequence` then
  sees `head_ref != original_ref` and answers `rewound: false`. The user can no longer abort
  back to where they started.
- **Delete:** libgit2 refuses to delete the current branch. But once HEAD has moved (for
  example with `git switch` in a terminal), the branch the stored sequence names can be
  deleted.
- **Stash save:** this carries a resolved step away from the index while `CHERRY_PICK_HEAD`
  stays behind. That is the reason the rebase gate refuses it.

### #172: the undo store between two processes

`worker/undo.rs` (`UndoRecorder`) and `worker/undo_store.rs`:

- `settle(marker)` runs before every command. It `reload_if_changed`s (by mtime) and drops a
  pending entry whose marker differs from the current one, then saves.
- `finish` turns the active entry into `pending` and saves **without reloading first**. When an
  operation ends, it reloads and records `pending`.
- **Settle race:** process A's `RebaseContinue` clears `rebase.json` inside the command, then
  A's `finish` records the pending entry. If process B runs any command in that gap (the file
  watcher triggers reads), B's `settle` sees marker `None`, drops `pending` and saves. A's
  `finish` then reloads, finds nothing and records nothing.
- **Lost update:** every save is load-modify-save with no exclusion. Two writers lose one
  entry (last writer wins), and `write_anchor`/`delete_stale` can interleave their ref writes.
- **mtime reload:** `reload_if_changed` misses a second write within the same mtime tick
  (1–2 s on HFS+, FAT and SMB).
- **Stale marker:** `merge <HEAD> <MERGE_HEAD>` is identical for a merge that was aborted in a
  terminal and started again with the same heads. The old pending entry, and the snapshot taken
  before the first merge, are reused. The same holds for `cherry-pick`/`revert` markers.
- **Temp names:** `undo.json.<pid>.tmp` (`undo_store.rs:192`) and `sequence.json.<pid>.tmp`
  (`sequence_store.rs:156`) are per process. Two workers in one process can write the same temp
  file. `rebase_store.rs:228` already uses `<pid>.<counter>`.

Platform facts for a lock. The toolchain is stable 1.97, and `std::fs::File::lock`/`try_lock`
have been stable since 1.89. They use `flock(LOCK_EX)` on Linux/macOS and `LockFileEx` on
Windows. A probe on Linux (deleted afterwards) confirmed three things:

- two separate `File` handles in **one** process exclude each other (`WouldBlock`);
- dropping the holder releases the lock;
- a thread that panics while holding the lock releases it when it unwinds.

The OS also releases the lock when a process dies, `kill -9` included. Using it adds no
dependency.

### #161 / #167: the case-collision guard

`worktree_guard.rs`:

- `prepare_forced_checkout(repo, target, paths)` (reset --hard, discard, merge abort, undo,
  rebase restore) refuses a collision of `target` **unless HEAD's tree holds the identical
  one**. It compares every collision (`find_case_collisions`), as the #156 follow-up already
  decided.
- `refuse_linked_folders(repo, incoming)` (switch, checkout, merge ff/non-ff, squash, pull ff,
  cherry-pick, revert, sequence steps and rewind, rebase start/steps, stash apply/pop) refuses
  the **first** collision across the incoming sources (`find_case_collision_in`), with no HEAD
  exemption. Merged indexes (`merge_commits`, `cherrypick_commit`) and stash trees are full
  snapshots that contain HEAD's entries. So with HEAD holding link `D` + `d/x`, every merge-like
  operation is refused (#167), and so is a fast-forward or switch to a tree that keeps the
  pair. The comment at `worktree_guard.rs:717-719` and `docs/ARCHITECTURE.md` say such a repo
  "isn't locked", which is wrong today.
- **#161:**
  - Rebase abort and the start rollback call `restore_original_branch`, which calls
    `prepare_forced_checkout` with the branch tree as the target and the detached rebase HEAD
    as the baseline. If the original tree holds a collision that the rebased HEAD no longer
    holds (for example a step deleted the link), the abort is refused.
  - `abort_sequence` rewinds with `refuse_linked_folders(Checkout(original tree))`, which
    refuses any collision of the original tree.
  - Either way the user is stuck mid-operation, even though they only want to go back to a
    tree they already had.

Safety of exempting a collision. The collision check is a second line of defence. The first
is the on-disk scan in `refuse_linked_folders` and `prepare_forced_checkout`: it `lstat`s every
ancestor folder of everything HEAD, the index and the incoming sources hold. On a
case-insensitive file system, `lstat("d")` answers with the link `D`. The `tracked_as_link`
exemption compares exact case, so a write below `d/` through an on-disk `D` is refused with
`LinkedFolder` (for safe operations) or the link is cleared first (for forced ones). This holds
whether or not the collision check runs (spec `2026-10-28-security-followups-design.md` §1.1).
On Linux with `core.ignorecase=true`, which is how tests simulate case folding, both `D` and
`d/` exist on disk and there is nothing to write through. Only CI's macOS/Windows probe
(`tests/case_insensitive_links.rs`) exercises the real file system.

## Design

### 1. #170: a rebase starts only from a clean tracked tree (D1-A)

`git_core::rebase::start_rebase`, after `validate_plan` and the detached-HEAD check and
**before** any checkout:

```rust
/// Tracked local changes a rebase would discard on rollback or Abort: any staged change,
/// and any unstaged change to a tracked file (modified, deleted, type change, conflicted).
/// Untracked and ignored files are not counted (git doesn't either); onto's checkout refuses
/// an untracked file in its way. Submodule changes are ignored, as `git rebase` ignores them.
fn local_changes(repo: &Repository) -> Result<Vec<String>, RebaseError>;
```

It uses `repo.statuses` with `include_untracked(false)`, `include_ignored(false)` and
`exclude_submodules(true)`, keeps `INDEX_*`, `WT_MODIFIED`, `WT_DELETED`, `WT_TYPECHANGE`,
`WT_RENAMED` and `CONFLICTED`, and sorts the result by path. If the list is non-empty, the call
returns the new `RebaseError::LocalChanges { paths: Vec<String> }`. Nothing has been written at
that point, so the worker's undo recorder sees `before == after` and records nothing.

Copy (D2-A), in a `const` and asserted verbatim once:

- 1 path: "You have uncommitted changes to {path}. Commit or stash them before starting a
  rebase."
- n > 1 paths: "You have uncommitted changes to {first} and {n-1} other file(s). Commit or stash
  them before starting a rebase." (`file` for 1, `files` otherwise.)

The rest stays unchanged:

- The rollback (`restore_original_branch` after a failed `advance`) stays forced. With a clean
  tracked tree at start, it only discards what the rebase itself wrote.
- Abort stays forced: `git rebase --abort` parity for edits made during a pause (the accepted
  risk from #169).
- The frontend needs no change. `useMergeRebaseActions.startRebase` runs through
  `runMutation`, so the error reaches the existing error banner, and the planner stays open
  because `rebaseOnto` is only cleared on success.

Not checked: Continue. Edits made during a pause are the user's work on the rebase, and the
rules already cover them (bug 1 from #169: a refused re-apply keeps the state).

### 2. #171: a paused sequence protects its branch (D4-A)

`gate_during_operation(command, scope: &GateScope)`:

```rust
pub(super) struct GateScope {
    /// Short name of the branch the stored operation will return to or move: the rebased
    /// branch, or a sequence's `OriginalRef::Branch`. Delete, rename and worktree-add on it are refused.
    pub branch: Option<String>,
    /// Stash save is refused (it would carry a paused step's index away).
    pub refuse_stash_save: bool,
}
```

- For a stored rebase: `branch = rebased branch` and `refuse_stash_save = true` (today's
  behaviour).
- For a stored sequence: `branch = short name of original_ref` when it is
  `OriginalRef::Branch`, `None` for `Detached`/`Unknown`, and `refuse_stash_save = true`.
- `OperationProbe` gains `fn gate_scope(&self, repo) -> Option<GateScope>`, which returns
  `None` when nothing is stored. The worker loop calls it once per command in place of
  `rebased_branch` + `sequence_store.load`.
- The refusal text stays git-core's `OperationInProgress` ("Finish or abort the current
  merge/cherry-pick/revert/rebase first."), as for every other gated command.

### 3. #172: one advisory lock around undo bookkeeping, fresh markers, unique temp names

#### 3.1 `worker/atomic_file.rs`

```rust
/// Writes `bytes` to `file` atomically: a sibling temp file `<name>.<pid>.<n>.tmp` (n from a
/// process-wide counter, so two workers in one process never share it), `sync_all`, `rename`.
/// The temp file is removed on failure.
pub(super) fn write_atomically(file: &Path, bytes: &[u8]) -> std::io::Result<()>;
```

`undo_store`, `sequence_store` and `rebase_store` all use it, and their inline copies are
deleted. The file formats are unchanged.

#### 3.2 `worker/state_lock.rs` (D5-A)

```rust
pub(super) struct StateLock { file: PathBuf }           // <gitdir>/browsitory/state.lock
pub(super) struct StateGuard { _file: std::fs::File }   // released on drop
pub(super) enum Acquire { Held(StateGuard), Busy, Unavailable(std::io::Error) }
impl StateLock {
    pub fn open(repo: &Repository) -> Self;
    /// Polls `File::try_lock` (10 ms, doubling to 100 ms) until `wait` elapses.
    pub fn acquire(&self, wait: Duration) -> Acquire;
    pub fn try_acquire(&self) -> Acquire;                // = acquire(Duration::ZERO)
}
```

- The lock file is created with `create(true).truncate(false).write(true)` and **never
  deleted**. Deleting a locked file lets the next opener lock a new inode while the old holder
  still believes it has exclusion. Its content is empty and irrelevant.
- `repo.path()` is per worktree, so each linked worktree has its own lock, just as it has its
  own `undo.json` and anchor ref.
- **Staleness after a crash:** none to handle. The lock is an OS lock on an open handle, not
  the file's existence. A process that exits or is killed (`kill -9`, TerminateProcess)
  releases it, and so does a worker thread that panics (the guard drops while unwinding). A
  **hung** holder (for example a pinentry dialog during a signed Continue) keeps it, which the
  bounded wait covers (D6).
- `Unavailable`: the file system doesn't support locks (`ErrorKind::Unsupported`, some network
  mounts), or the folder can't be created. The worker then carries on unlocked, as today, and
  logs once per worker ("browsitory: undo history lock unavailable: …").

#### 3.3 What runs under the lock

The rule: **every write to `undo.json` happens under the lock, after a forced reload, and
re-checks its decision there.** A command that can end an operation holds the lock from before
it runs until its `finish`, so no other holder ever sees "the operation ended, but its entry is
not recorded yet".

In the worker loop, `locks_state(&command)` is true for:

- every command `undo::plan_for` returns `Some` for: recordable commands, which also covers
  everything that starts an operation and `Commit`/`AmendCommit`, which end a merge or pick;
- `RebaseContinue`, `AbortRebase`, `ContinueSequence`, `AbortSequence` and `AbortMerge`;
- `Undo` and `Redo`.

| Command | Lock |
|---|---|
| `locks_state` | `acquire(LOCK_WAIT)` before `settle`, held through the command and `finish`, dropped before the held replies are released. `Busy` after `LOCK_WAIT`: log once, run unlocked as today (D6-A). For `Undo`/`Redo` it is instead the error reply `UNDO_BUSY` (D7), and nothing is restored. |
| any other command (reads, fetch, staging, resolving) | `settle` and `finish` take `try_acquire` **only when they are about to write** (drop or record a pending entry). `Busy`: skip the write. A later command repeats it. Reads never wait. |

`LOCK_WAIT` is 10 s, or 300 ms under `#[cfg(test)]`. `UndoStore` gains `reload(&mut self,
&mut UndoStacks)` (unconditional) for the locked paths. `reload_if_changed` stays for the
unlocked `state()` read.

`UndoRecorder` changes. It holds the `StateLock`, and each method states whether its caller
holds the guard:

- `settle(repo, marker)`: unlocked fast path. If there is no pending entry, or its operation
  equals `marker`, do nothing. Otherwise take the lock (or reuse the caller's guard), `reload`,
  **recompute** the marker (the worker passes a closure), and drop the entry only if it is
  still stale.
- `begin(...)`: unchanged, except that it is called after a fresh `reload` when locked.
- `finish(...)`: under the lock, `reload` before every write. A new pending entry is added to
  the freshly loaded stacks (this fixes last-writer-wins). Recording a pending entry
  re-validates that it still exists after the reload.
- `undo`/`redo`: under the caller's guard, `reload` and then today's logic.
- `relabel`: unchanged; the `finish` that follows saves it.

Why the race is closed: A holds the lock from before `RebaseContinue` clears `rebase.json`
until its `finish` has recorded the entry. B's `settle` finds marker `None` with a pending
entry, so it needs the lock. A busy lock means B skips. B's next `settle` reloads, finds the
entry recorded and the pending entry gone, and does nothing.

#### 3.4 Fresh markers for git's own operations (D8-A)

`OperationProbe::marker` appends the state file's modification time, in nanoseconds since the
epoch, for the states whose files libgit2/git write once per operation:

- `merge <HEAD> <MERGE_HEAD…> @<mtime of MERGE_HEAD>`
- `cherry-pick <HEAD> <CHERRY_PICK_HEAD> @<mtime of CHERRY_PICK_HEAD>`
- `revert <HEAD> <REVERT_HEAD> @<mtime of REVERT_HEAD>`

The suffix is omitted when the mtime can't be read.

- Nothing rewrites these files while their operation is paused. Staging, resolving and
  `MERGE_MSG` edits don't touch them, and they are deleted when the operation ends. So the
  marker stays constant for the operation's life, and an abort followed by an identical
  restart gets a new marker.
- Both processes stat the same file, so they agree on the marker.
- The `rebase …` and `sequence …` markers are unchanged. While Browsitory's own file exists, an
  identical rebase or sequence can't start again (the gate refuses it), and their files are
  rewritten every step, so their mtime can't be used.
- Upgrade: a merge/pick pending entry written by the previous build carries the old marker
  without `@…`. It is dropped once by `settle`, so that operation becomes unrecordable, as
  after a terminal abort. This is noted in the CHANGELOG.
- On file systems with 1–2 s mtime resolution, an abort and identical restart within the same
  tick while the app is closed still collide. That is an accepted residual.

### 4. #167: collisions HEAD already holds are not "brought in" (D9-A)

```rust
/// Every case collision across `sources` (as `find_case_collision_in`, but all of them).
fn find_case_collisions_in<P>(sources: &[GuardSource], paths: Option<&[P]>) -> Result<Vec<CaseCollision>, git2::Error>;

/// The collisions an operation may write again: HEAD tree's, plus `also_known`'s (a tree the
/// operation restores, §5). Exact `(link, folder)` spelling.
fn known_collisions<P>(repo, paths: Option<&[P]>, also_known: Option<&git2::Tree>) -> Result<Vec<CaseCollision>, git2::Error>;
```

- `refuse_linked_folders` (after its link scan, unchanged) computes
  `find_case_collisions_in(incoming)` and refuses the first one not in `known_collisions`.
  Order is preserved, so the reported collision is deterministic.
- `prepare_forced_checkout` replaces its inline HEAD comparison with `known_collisions`,
  keeping the same behaviour.
- Effects:
  - An unrelated merge (non-ff and ff), cherry-pick, revert, sequence step, stash apply/pop,
    pull ff and rebase (start and steps) work with HEAD holding `D` + `d/x`.
  - A switch or checkout to a tree that keeps the **identical** pair also works. This is a
    behaviour change: today it is refused.
  - A **second** collision, or one spelled differently from HEAD's, is still refused.
- The comment at `worktree_guard.rs:717-719` becomes true as written. `docs/ARCHITECTURE.md`
  "Worktree writes and symlinks" is updated to say which collisions are exempt and why the
  disk scan keeps the exemption safe.

### 5. #161: an abort may restore the collisions of the tree it returns to

Both functions gain an `also_known: Option<&git2::Tree>` variant:

- `refuse_linked_folders_known(repo, incoming, also_known)`;
- `prepare_forced_checkout_known(repo, target, paths, also_known)`.

The existing names are thin wrappers that pass `None`, so the other callers don't change.

- `rebase::restore_original_branch(repo, branch_ref, original_tip)` passes
  `original_tip`'s tree as `also_known`. Its two callers (Abort with `restored: true`, and the
  start rollback) only run when the branch is still at `original_tip`, so this is exactly
  "the tree the user had". The link-hazard clearing still runs.
- `pick::abort_sequence`'s rewind passes `original_head`'s tree as `also_known`. The disk scan
  still runs, and the checkout stays safe (not forced).
- Not changed: `undo::restore`'s `refuse_case_collision` on its target work tree (undo writes
  raw files itself, see "Out of scope").

## Open decisions (provisional choice in **bold**; the plan implements it)

**D1. What `start_rebase` refuses (#170).**

- **A. Any tracked local change: staged, or an unstaged change to a tracked file. Untracked and
  ignored files and submodule changes are allowed.** This matches `git rebase`'s
  `require_clean_work_tree`. Rollback and Abort can stay forced because the only thing they can
  discard is the rebase's own output, or edits made during a pause (git parity).
- B. Refuse only local changes that overlap the plan's paths (the HEAD→onto diff plus every
  non-Drop step's diff), and always refuse staged changes (libgit2 can't pick over them, probe
  case 2b). Rollback and Abort then force-checkout **only** those paths
  (`CheckoutBuilder::path` with `disable_pathspec_match`, plus `prepare_forced_checkout` with
  the same paths), so unrelated unstaged edits survive.
  - Cost: computing the path set at start and again at Abort (from `plan`, `original_tip`,
    `onto`, the current index and HEAD).
  - Cost: libgit2's pathspec matching is roughly O(entries × paths), which is slow for a
    rebase across thousands of changed paths.
  - Cost: Abort no longer matches `git rebase --abort`: a file changed during a pause outside
    those paths stays modified or staged afterwards.
- C. A with an "Stash and rebase" offer: stash, rebase, then pop after Done or Abort. That needs
  persisted autostash state (a `rebase.json` v2) and handling for a conflicting pop. It is
  better as a follow-up feature.
- D. No refusal; make rollback and Abort non-forced. A refused safe checkout would then leave
  the user on a detached HEAD with an Error stop, which is worse than refusing up front.

Recommendation **A**: the probe shows the loss isn't limited to overlapping paths. A is git's
rule, simple to reason about, and makes every forced restore safe for pre-existing work. B is
the fallback if refusing unrelated edits turns out too strict in practice. C can follow either.

**D2. Copy for the refusal (#170).**

- **A. "You have uncommitted changes to {path}. Commit or stash them before starting a
  rebase." / "… to {first} and {n} other file(s). …"**
- B. git's wording: "Cannot rebase: You have unstaged changes. Please commit or stash them."
  It is familiar, but names no file and says "unstaged" for staged changes too.
- C. A generic message with every path listed. A long list breaks the error banner.

Recommendation **A**.

**D3. Where the refusal shows (#170).**

- **A. Only as the start error: the planner stays open and the error banner shows the text.**
  No frontend change.
- B. Also disable "Start rebase" in the planner, with the text as `disabledReason`, while
  `status` has tracked changes. This adds a frontend dependency on status freshness: a change
  made in an editor without a refresh would still reach the backend refusal.

Recommendation **A**. B can follow if users ask for it.

**D4. What a paused sequence protects (#171).**

- **A. Delete, rename and worktree-add on the sequence's original branch, plus stash save. This
  is the same set as a rebase.**
- B. Only delete and rename, as #171 asks.
- C. Allow a rename and rewrite `sequence.json`'s `original_ref` to follow it.

Recommendation **A**: parity with the rebase gate, one rule for both. C adds a store write for
a rare case.

**D5. Lock mechanism (#172).**

- **A. `std::fs::File::lock`/`try_lock` (std since 1.89; flock / LockFileEx). No new
  dependency; released by the OS on exit or crash.**
- B. The `fs4` crate (MIT OR Apache-2.0), which needs a `docs/LICENSE_COMPLIANCE.md` row and
  gives nothing std doesn't.
- C. An `O_EXCL` lock file (as git's `index.lock`). It goes stale after a crash and needs
  PID/age heuristics, which differ per OS.

Recommendation **A**.

**D6. A command that can't get the lock within `LOCK_WAIT` (#172).**

- **A. Wait up to 10 s, then run the command unlocked (today's behaviour) and log once. Undo/Redo
  answer with an error instead (D7).** A hung holder never blocks the user's git work for more
  than 10 s. The cost is that, only under that contention, the old race is possible again.
- B. Wait indefinitely. This is always correct, but a pinentry dialog left open in the other app
  freezes this one.
- C. Refuse every locked command after the wait, with a message. This is safe but needs a busy
  reply for every command type (a large `refuse_busy` match), and it fails user commands for a
  bookkeeping reason.

Recommendation **A**.

**D7. Copy when Undo/Redo can't get the lock.**

- **A. "Another Browsitory window is changing this repository's undo history. Try again in a
  moment."** It is sent as the command's `Err`, so it shows in the existing error toast with no
  wire change.
- B. A new `UndoOutcome::Busy` variant. That is a wire change in Tauri, the sidecar and
  `RepoClient`, and needs a frontend toast.

Recommendation **A**.

**D8. Telling an identical restarted merge/pick apart (#172).**

- **A. Add the state file's mtime (ns) to the marker.**
- B. Keep the marker; store `created_at` in the pending entry and treat it as stale when the
  state file is newer. This keeps the marker format, but adds a field and a clock comparison
  across processes.
- C. Document the stale marker only. Redo recovers the newer state.

Recommendation **A**: one string, no format change, and the same value in both processes.

**D9. Which collisions count as "already in HEAD" (#167).**

- **A. Identical `(link, folder)` spelling as in HEAD's tree, as `prepare_forced_checkout`
  already does. The disk scan stays the guard against writing through a link.** This also
  allows a switch to a tree with the identical pair (behaviour change, §4).
- B. A, and also require that the operation doesn't change the link entry (mode and oid) or any
  entry below the colliding folder. This is stricter and still refuses a merge that edits
  `d/x`, but it needs per-entry diffs against HEAD for every incoming source.
- C. Fold-equal pairs (any spelling that folds to HEAD's pair). This is looser, and refused by
  the user's "only identical" rule.

Recommendation **A**, as the user asked. B stays available if the CI case-insensitive probe
(Task A4) shows any row writing outside.

**D10. E2E.**

- **A. One Tauri case in `e2e/specs/rebase.spec.ts`: a modified tracked file, then "Start
  rebase" shows the error and the file keeps its content. No VSCode E2E: the VSCode suite has
  no rebase spec, and the backend path is shared.**
- B. No E2E. The Rust tests cover it.

Recommendation **A**. #172 and #167 have no UI surface to drive black-box (two app instances
can't run under one tauri-driver session).

## Testing

All Rust tests use real temp-dir repos, and each new test binary starts with
`test_support::hermetic_git!()` (or `mod common;`). Case folding is simulated with
`core.ignorecase=true` on Linux. Tests that need HEAD to hold a collision first switch to it
with `core.ignorecase=false`, so they are `#[cfg(target_os = "linux")]` like the existing ones.
Tests that use `gpg.program=/bin/false`, read-only folders or symlinks are `#[cfg(unix)]`.

- **git-core `tests/rebase.rs` (#170):**
  - start refuses an unstaged change, a staged change and a deleted tracked file. HEAD, the
    branch, the index and the file are untouched, and there is no detach;
  - start allows untracked files;
  - the copy for 1 and n paths;
  - `#[cfg(unix)]`: a failed first step (gpg `/bin/false`) on a clean tree rolls back to the
    branch, with no `CHERRY_PICK_HEAD` and the work tree equal to the branch tree.
  - Existing tests that start with a dirty tree are fixed by committing first.
- **git-core `tests/case_collision.rs` (#167, #161, Linux):**
  - with HEAD holding the pair: unrelated non-ff merge, ff merge, cherry-pick, revert, stash
    apply, pull ff, rebase start and steps, switch to a branch with the identical pair;
  - still refused: a second collision via merge, and via cherry-pick;
  - rebase abort restores an original tree holding the pair after a step removed it;
  - sequence abort rewinds to an original holding the pair after a step removed it.
- **git-core `worktree_guard.rs` unit tests:**
  - `find_case_collisions_in` returns every collision across sources, in order;
  - a collision spelled differently from HEAD's is not exempt;
  - `also_known` exempts its own collisions only.
- **git-core `tests/case_insensitive_links.rs` (CI macOS/Windows; self-skips elsewhere):** new
  rows with HEAD holding the pair, created by a raw libgit2 checkout:
  - an unrelated merge, cherry-pick and stash apply through git-core;
  - a merge that changes `d/x`.
  - Only "nothing outside changed" is asserted, and the row table is printed.
- **repo-service:**
  - `atomic_file`: unique temp names; 8 threads × 50 writes to one file never fail and leave no
    `*.tmp`.
  - `state_lock`:
    - a second handle in the same process is `Busy`, then `Held` after the first drops;
    - `acquire` times out;
    - a panicking holder thread releases;
    - a killed child process releases (self-exec test, portable);
    - the file survives release.
  - `undo` (two recorders on one repository = two processes):
    - settle doesn't drop a pending entry while the other holds the lock;
    - finish reloads before adding a pending entry (no lost entry);
    - recording in turn keeps both entries;
    - Undo replies `UNDO_BUSY` while the lock stays held.
  - `gate`:
    - the marker carries the mtime and is stable through staging;
    - a rewritten `MERGE_HEAD` changes the marker;
    - the gate scope for a sequence on a branch, a detached sequence and a rebase.
  - worker (`worker/mod.rs` tests):
    - the settle race, through a `#[cfg(test)]` pause-before-finish hook on worker A while
      worker B runs a read;
    - an identical merge restarted outside doesn't reuse the old pending entry;
    - a locked command proceeds after `LOCK_WAIT` while the test holds the lock;
    - the sequence gate refuses delete/rename/worktree on its branch and stash save, while
      other branches stay manageable;
    - a refused rebase start records no undo entry.
- **vscode-sidecar `protocol_roundtrip.rs`:** two sidecar processes on one repository. A
  conflicted merge started in the first and committed in the second is one undo entry, seen
  by both.
- **Tauri E2E (D10-A):** `rebase.spec.ts`, "starting a rebase with uncommitted changes is
  refused".

## Docs

- `docs/ARCHITECTURE.md`:
  - "Undo stack": the state lock (what it covers, why it's never deleted, crash behaviour,
    `LOCK_WAIT` fallback), forced reload under the lock, the mtime in git-state markers;
  - "Interactive rebase": start refuses tracked local changes;
  - "Cherry-pick sequences": the branch and stash gate;
  - "Worktree writes and symlinks": the HEAD and `also_known` exemptions, and the disk-scan
    argument.
- `docs/USER_GUIDE.md`:
  - "Rebase": commit or stash first;
  - "Rewriting and moving commits" (sequences): branch delete/rename is refused while paused.
- `CHANGELOG.md`: `### Rebase/undo safety` (wording in the plan's Task Z1).
- No `docs/LICENSE_COMPLIANCE.md` change (D5-A adds no dependency).

## Risks

- **Stricter rebase start (D1-A).** Users who used to rebase with unrelated edits now have to
  commit or stash first. Mitigation: clear copy, and D1-B/C stay open.
- **False "changes" from filters.** `core.autocrlf` or filemode differences can make libgit2
  report a file as modified, so the start is refused. git does the same. The copy names the
  file.
- **Lock held across a long command.** A pull (network) or a signed commit waiting on pinentry
  holds the lock. The other process's locked commands wait up to 10 s and then run unrecorded
  (D6-A); its reads never wait.
- **Unsupported locks** (some network file systems): behaviour falls back to today's. This is
  logged.
- **Marker format change** drops a pending merge/pick entry written by the previous build, once
  (§3.4).
- **Exemption relies on the disk scan (D9-A).** A real case-insensitive file system is only
  covered by the CI probe. If a new row writes outside, switch to D9-B before merging.
- **Wider `refuse_linked_folders` signature change** touches about 12 call sites. The wrappers
  keep their names, so only rebase and pick change.

## Unrelated findings (not fixed here)

1. **Undo label for a rebase carries the full commit id.** `plan_for` builds `fit(format!("Rebase
   onto {onto}"))` without `short()`, so the stack shows "Rebase onto
   707e824e1af1b266aaa5c4faf7517e586b174183" (probe case 4). The frontend passes the full oid
   from "Rebase onto here".
2. **A failed command that destroyed work is recorded under the operation's name.** A failed
   StartRebase is recorded as "Rebase onto …", even though no rebase happened. This is useful
   (it is what made case 4 recoverable), but the label suggests success. Once #170 is fixed
   this mostly disappears; a "(failed)" suffix could be a follow-up.

## Out of scope

- An autostash option for rebase (D1-C).
- Undo restoring a snapshot whose work tree holds a collision HEAD lacks
  (`undo::restore`'s `refuse_case_collision`). By #161's logic it restores a known state too,
  but its raw writes need their own review.
- Path-limited Abort (D1-B), unless chosen.
- A UI for "another window is busy".
- A lock around `rebase.json`/`sequence.json` writes outside locked commands. Every command
  that writes them is a `locks_state` command.
