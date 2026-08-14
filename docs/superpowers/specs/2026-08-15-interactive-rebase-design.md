# Interactive Rebase Design

Status: Approved

## Context

Branch management, stash, blame, the multi-branch commit graph, and merge (see
`docs/superpowers/specs/2026-08-12-branch-management-design.md`,
`docs/superpowers/specs/2026-08-12-stash-design.md`,
`docs/superpowers/specs/2026-08-13-blame-design.md`,
`docs/superpowers/specs/2026-08-13-commit-graph-design.md`, and
`docs/superpowers/specs/2026-08-14-merge-design.md`) shipped the first five of Phase 2's six
subsystems. This spec covers the sixth and last: interactive rebase.

**Goals:**
- Rebase the current branch onto any earlier commit, triggered by right-clicking that commit in
  the commit graph ("Rebase onto here"), matching the existing "Branch from here" pattern.
- A reorderable plan of every commit between the chosen commit and the current branch's HEAD
  (oldest-first, matching actual replay order), each with a per-commit action: Pick, Reword,
  Squash, Fixup, Drop, Edit — the standard `git rebase -i` toolkit minus `exec`.
- Reword's new message and a Squash/Fixup group's combined message are both decided during
  planning, before the rebase starts — not as an execution-time pause. Squash/fixup message
  combination is pure text, independent of tree content, so there's no reason to defer it.
- Conflicts during a rebase step reuse the merge feature's conflict-resolution UI/backend
  unchanged — a cherry-pick conflict has the exact same `IndexConflict` shape as a merge
  conflict.
- An Edit pause needs no new UI: the cherry-picked changes just sit staged in the working tree
  like any other uncommitted change, handled by the existing Stage/Unstage/diff UI, plus one new
  "Continue Rebase" action to move on.
- Abort restores the branch to exactly where it was before the rebase started.

**Non-goals (explicitly deferred):**
- `exec` (run an arbitrary command between steps) — a power-user CLI feature, not a natural fit
  for a GUI planner.
- Rebasing onto a different branch's tip (`git rebase <upstream>` in the "move my branch onto
  someone else's work" sense) — this pass is "restructure my own branch's history down to some
  earlier point," triggered from a commit in the graph, not a branch picker. A future pass could
  add that as a second trigger without changing the underlying engine.
- Persisting rebase state across an app restart — session-scoped only (an explicit choice; see
  Architecture). Closing the app mid-rebase leaves the repo in whatever partial cherry-pick state
  it was in, recoverable via the CLI, same class of risk every other in-progress-operation
  feature in this app already carries (e.g. an interrupted merge).
- Reordering/dropping commits that are already merge commits in the selected range — the plan
  only lists commits reachable via first-parent from HEAD down to the chosen commit; a range
  containing a merge commit is out of scope for this pass (real `git rebase -i` handles this via
  `--rebase-merges`, itself a whole additional design surface).

## Architecture

### Why a hand-rolled cherry-pick loop, not git2's `Repository::rebase`

git2's built-in `Rebase` (verified against the vendored source,
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/git2-0.21.0/src/rebase.rs`) is an
`Iterator` that applies commits in a **fixed order** determined entirely at `repo.rebase(...)`
init time from walking the branch/upstream ancestry — there is no way to reorder, drop, or
squash entries through this API; the todo list isn't constructible or mutable in the way `git
rebase -i`'s editable todo file is. Since reorder/drop/squash/edit are explicit goals, this pass
builds its own loop directly on `Repository::cherrypick` (the real, working-tree-updating
primitive — not `cherrypick_commit`, which is the in-memory-only variant analogous to `merge`
vs. `merge_commits`) and `git-core::commit`'s existing commit-creation logic.

`Repository::cherrypick(&self, commit, options) -> Result<(), Error>` applies a commit's changes
onto the current working tree/index exactly like `Repository::merge` does for a merge — including
writing conflict markers into the working tree and setting `index.has_conflicts()` when there's a
real conflict. Verified: `RepositoryState::CherryPick` is a distinct enum variant from
`RepositoryState::Merge` (`git2-0.21.0/src/lib.rs:290,319`), so `git-core::commit`'s existing
`is_merging` check (`repo.state() == RepositoryState::Merge`, added for the merge feature) is
unaffected by a rebase step — a rebase-step commit correctly gets a single parent, not an
accidental extra one.

### `git-core` addition: `rebase.rs`

Same shape as other modules: a `thiserror` `RebaseError` enum, tested against real temp-dir repos
(`crates/git-core/tests/rebase.rs`).

```rust
pub struct RebasePlanCommit {
    pub id: String,
    pub short_id: String,
    pub summary: String,
    pub author_name: String,
    pub timestamp: i64,
}

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
    // Set only on a "group leader" (a Pick/Reword/Edit entry immediately followed by one or
    // more Squash/Fixup entries) — the message to use for the resulting combined commit. `None`
    // on an entry with no trailing squash/fixup group, in which case the original message (or
    // Reword's) is used as-is. `None` and meaningless on Squash/Fixup entries themselves — they
    // never produce their own commit.
    pub combined_message: Option<String>,
}
```

The plan's first entry can never be `Squash`/`Fixup` — there is no preceding entry *within the
plan* for it to attach to (`onto` itself, the commit being rebased onto, is not a valid squash
target, matching real `git rebase -i`'s own "cannot 'squash' without a previous commit"
restriction). `RebasePlanner` disables those two options on the first row; `start_rebase`
validates this server-side too (a `RebaseError::InvalidPlan` variant) rather than trusting the
frontend never sends it.

```rust
pub enum RebaseStepResult {
    Conflicted { files: Vec<String> },
    PausedForEdit,
    Advanced,
    Done,
}

pub struct RebaseState { /* opaque to callers outside git-core */ }

pub fn commits_since(repo: &Repository, onto: &str) -> Result<Vec<RebasePlanCommit>, RebaseError>;
pub fn start_rebase(repo: &Repository, onto: &str, plan: Vec<RebasePlanEntry>) -> Result<(RebaseState, RebaseStepResult), RebaseError>;
pub fn rebase_continue(repo: &Repository, state: &mut RebaseState) -> Result<RebaseStepResult, RebaseError>;
pub fn abort_rebase(repo: &Repository, state: RebaseState) -> Result<(), RebaseError>;
```

- `commits_since` walks a `Revwalk` pushed from `HEAD` with `onto`'s commit `hide`den (verified:
  `Revwalk::hide(&mut self, oid: Oid) -> Result<(), Error>` exists), producing the commits
  strictly between `onto` (exclusive) and `HEAD` (inclusive) — oldest-first (the walk's natural
  newest-first order, reversed), matching actual replay order. This seeds the frontend planner's
  initial list (default action `Pick` for every entry, in this order) before the user edits it.
- `start_rebase`/`rebase_continue` share one internal step-execution loop: for the plan entry at
  the current cursor, if `Drop` — skip it entirely (tip unchanged), advance the cursor,
  continue the loop. Otherwise `repo.cherrypick(&commit, None)`; if the resulting index has
  conflicts, return `Conflicted { files }` immediately (the same file-listing logic
  `merge::start_merge` already uses) without advancing the cursor — the *same* plan entry is
  finished once the caller resolves and calls `rebase_continue` again. If clean and the entry is
  `Squash`/`Fixup`, the changes are already staged in the index; don't commit yet, advance the
  cursor, continue the loop (the actual commit happens when the group's leader's turn to land is
  reached — practically: keep applying the group's remaining entries on top of the same
  accumulating index and defer committing until the next non-squash/fixup entry or `Done`, using
  the leader's `combined_message`/message). If clean and the entry is `Edit`, return
  `PausedForEdit` without committing — the working tree/index is left exactly as the cherry-pick
  produced it, ready for the caller to amend via the *existing* stage/unstage/diff machinery.
  Otherwise (`Pick`/`Reword`, or a `Squash`/`Fixup` group's leader once its group's last member
  has landed cleanly) commit via `git-core::commit::commit` with the entry's message (original,
  `Reword`'s, or the group's `combined_message`), advance the cursor, continue the loop. Once the
  cursor passes the end of the plan, move the original branch ref to the final landed commit
  (checkout-then-`set_target`, the same safe ordering `merge::start_merge`'s fast-forward path
  already uses) and return `Done`.
- `abort_rebase` consumes the `RebaseState` (ending it), hard-resets the working tree/index back
  to the branch's original tip (recorded when `start_rebase` began), and clears
  `RepositoryState::CherryPick` via `repo.cleanup_state()`.
- `RebaseState` is intentionally opaque outside `git-core` — the worker thread holds it, nothing
  else needs to inspect its fields directly.

### `tauri-app`: session-scoped rebase state, not a new subsystem's worth of `Command`s

The `Worker` thread's closure gains one more piece of owned state alongside `repo`:
`let mut rebase_state: Option<git_core::rebase::RebaseState> = None;`. Five new `Command`
variants (`CommitsSince`, `StartRebase`, `RebaseContinue`, `AbortRebase`, `GetRebaseProgress`)
follow the exact existing pattern. `GetRebaseProgress` is what lets the frontend's persistent
rebase panel know it's mid-rebase after a refresh (analogous to merge's `get_merge_message`) —
it reports whether `rebase_state` is `Some` and, if so, a step index/total for the "Step 3 of 7"
display.

### `RepoClient` / frontend IPC

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

commitsSince(commitId: string): Promise<RebasePlanCommit[]>;
startRebase(onto: string, plan: RebasePlanEntry[]): Promise<RebaseStepResult>;
rebaseContinue(): Promise<RebaseStepResult>;
abortRebase(): Promise<void>;
getRebaseProgress(): Promise<{ currentStep: number; totalSteps: number } | null>;
```

### Frontend state and components

`useAppState.ts`'s `AppState` gains `rebaseProgress: { currentStep: number; totalSteps: number }
| null`, fetched via `getRebaseProgress()` in `refresh()`'s existing `Promise.all`. `null` means
no rebase in progress.

**`CommitGraph.tsx`**'s existing right-click context menu ("Branch from here") gains "Rebase onto
here", calling a new `onRebaseFromCommit(commitId)` prop that fetches `commitsSince(commitId)`
and opens the new **`RebasePlanner`**.

**`RebasePlanner.tsx`** (new component, rendered as an overlay/modal similar to the existing
create-branch draft UI): a reorderable list (up/down buttons — drag-and-drop is a nice-to-have,
not required for this pass) of `RebasePlanCommit`s, oldest-first. Each row has an action selector
(Pick/Reword/Squash/Fixup/Drop/Edit); selecting Reword reveals an inline message field;
attaching one or more Squash/Fixup rows beneath a Pick/Reword/Edit row reveals that leader row's
"combined message" field, pre-filled by concatenating the leader's message with every `Squash`
entry's original message in the group (`Fixup` entries' messages are never included) — editable
before starting. "Start Rebase" calls `onStartRebase(ontoCommitId, plan)`.

**Rebase-in-progress panel**: rendered in place of (or alongside, layout TBD at implementation
time) `CommitBox` whenever `appState.state.rebaseProgress !== null` — shows "Step
`{currentStep}` of `{totalSteps}`", a "Continue Rebase" button (`onRebaseContinue`), and "Abort
Rebase" (`onAbortRebase`). A conflicted step's file renders in the existing Uncommitted Changes
list with the existing `Conflicted` kind, opening the existing `ConflictResolutionPane` — no new
conflict UI. An `Edit`-paused step's changes render as ordinary staged/unstaged entries in the
same list, handled by the existing Stage/Unstage/diff UI; "Continue Rebase" is what actually
commits them once the user's satisfied.

### Error handling

No new plumbing: rebase errors flow through the same `Result<T, String>` → rejected promise →
`state.error` → inline banner path every other feature already uses.

### Testing

- `crates/git-core/tests/rebase.rs`: `commits_since` returns the right oldest-first list for a
  multi-commit branch; a clean reorder (two independent-file commits swapped) produces the
  expected final tree/history; a squash with a custom combined message produces one commit
  carrying that message; a drop is genuinely absent from the resulting history; a conflicting
  pick returns `Conflicted`, and resolving + calling `rebase_continue` lands it and proceeds; an
  `Edit` pause returns `PausedForEdit`, and staging an amendment + `rebase_continue` commits the
  amended state; `abort_rebase` mid-plan restores the branch to its exact original tip commit,
  working tree, and index.
- `crates/tauri-app/src/worker.rs`: thin wiring round-trip tests for each new command, matching
  the economical scope every other feature's worker tests already use.
- `frontend/src/components/RebasePlanner.test.tsx` (new): reordering updates the plan order,
  selecting Reword reveals and wires the message field, attaching a Squash row reveals the
  leader's combined-message field pre-filled correctly, "Start Rebase" calls `onStartRebase` with
  the assembled plan.
- `frontend/src/components/CommitGraph.test.tsx` additions: the context menu's new "Rebase onto
  here" entry calls `onRebaseFromCommit` with the right commit id.
- One new E2E flow (`e2e/specs/`): reorder two commits, attach a squash with a custom combined
  message, drop one commit, hit and resolve a conflict on another step, finish, and confirm the
  resulting linear history in the commit graph matches the plan (right commit count, right
  messages, no merge-commit fan-out).
