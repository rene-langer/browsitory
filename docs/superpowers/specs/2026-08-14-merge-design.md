# Merge (with Conflict Resolution) Design

Status: Approved

## Context

Branch management, stash, blame, and the multi-branch commit graph (see
`docs/superpowers/specs/2026-08-12-branch-management-design.md`,
`docs/superpowers/specs/2026-08-12-stash-design.md`,
`docs/superpowers/specs/2026-08-13-blame-design.md`, and
`docs/superpowers/specs/2026-08-13-commit-graph-design.md`) shipped the first four of Phase 2's
six subsystems. This spec covers the fifth: merging one local branch into the current branch,
including in-app conflict resolution. Phase 2's remaining subsystem (interactive rebase) is out
of scope here, its own future spec.

**Goals:**
- Merge any other local branch into the current branch, triggered from `BranchSwitcher`'s
  existing per-branch action row (alongside Switch/Rename/Delete).
- Fast-forward automatically when possible — matches plain `git merge`'s default. Otherwise
  perform a real merge: clean (no conflicts) or conflicted.
- Conflict resolution is per-hunk: a conflicted file shows each conflicting region with the two
  sides, and the user picks "ours", "theirs", or "both" per hunk — not whole-file-only, not an
  in-app text editor. Non-conflicting parts of the file (git2's own 3-way merge already resolves
  most of a typical conflicted file automatically) render as plain merged text, not as a hunk
  needing a decision.
- An "Abort merge" action while conflicts are unresolved, resetting the working tree/index back
  to pre-merge state.
- Once every conflict is resolved (or if the merge had none to begin with), committing reuses the
  existing `CommitBox`/`Commit` flow — a merge commit is exactly a normal commit with the extra
  parent already recorded by git2's merge machinery, not a separate "finish merge" action.

**Non-goals (explicitly deferred):**
- Remote branches / fetch — local branches only, matching every other Phase 2 subsystem's own
  scope decision so far.
- Merge strategies/options (recursive vs. ours vs. octopus, `-X` strategy flags) — git2's default
  merge behavior only.
- Cherry-pick, rebase, or squash-merge — separate operations, not this spec.
- A full-blown diff3 editor with manual text editing inside a conflict region — the per-hunk
  ours/theirs/both choice is the whole resolution surface for this pass.

## Architecture

### `git-core` addition: `merge.rs`

Same shape as other modules: a `thiserror` `MergeError` enum, tested against real temp-dir repos
(`crates/git-core/tests/merge.rs`).

```rust
pub enum MergeOutcome {
    UpToDate,
    FastForwarded,
    Merged,                        // no conflicts, ready to commit
    Conflicted { files: Vec<String> },
}

pub enum ConflictSegment {
    Clean { content: String },
    Conflict { ours: String, theirs: String },
}

pub fn start_merge(repo: &Repository, branch_name: &str) -> Result<MergeOutcome, MergeError>;
pub fn conflict_hunks(repo: &Repository, path: &str) -> Result<Vec<ConflictSegment>, MergeError>;
pub fn resolve_conflict(repo: &Repository, path: &str, resolved_content: &str) -> Result<(), MergeError>;
pub fn abort_merge(repo: &Repository) -> Result<(), MergeError>;
pub fn merge_message(repo: &Repository) -> Option<String>;
pub fn is_merging(repo: &Repository) -> bool;
```

- `start_merge` resolves `branch_name` to a `Commit` (same `resolve_start_point`-style lookup
  `branch.rs` already uses), wraps it as an `AnnotatedCommit` via `repo.find_annotated_commit`,
  and runs `repo.merge_analysis(&[&annotated])` first:
  - `ANALYSIS_UP_TO_DATE` → `UpToDate`, no further action.
  - `ANALYSIS_FASTFORWARD` (and not `ANALYSIS_UNBORN`) → move the current branch's ref straight
    to the target commit's `Oid` and `checkout_tree`, matching plain `git merge`'s default
    behavior — `FastForwarded`.
  - Otherwise → `repo.merge(&[&annotated], None, None)`, which performs a real 3-way merge,
    writes results into the working tree, and stages everything (conflicted paths included) into
    the index. Then check `repo.index()?.has_conflicts()`: `false` → `Merged` (working tree and
    index are ready to commit as-is); `true` → `Conflicted { files }`, `files` collected from
    `Index::conflicts()`.
- `conflict_hunks` looks up the file's `IndexConflict` (ancestor/our/their `IndexEntry`s) via
  `Index::conflicts()`, then calls `repo.merge_file_from_index(&ancestor, &our, &their, None)` —
  git2's own textual 3-way merge, which auto-resolves everything it safely can and marks the rest
  with `<<<<<<<`/`=======`/`>>>>>>>` markers in `.content()`. Parsing that marked text into a flat
  `Vec<ConflictSegment>` (plain runs of text between markers become `Clean`, marked blocks become
  `Conflict { ours, theirs }`) is the only custom logic this function does — everything upstream
  of that (the actual 3-way diff) is git2's, not hand-rolled.
  - **Add/delete conflicts** (a file present on only one side, or deleted on one side and
    modified on the other) have `None` for one or more of `ancestor`/`our`/`their` — there's no
    sensible textual 3-way merge for those, so `merge_file_from_index` isn't the right tool.
    `conflict_hunks` returns an error for this case (`MergeError::NotATextConflict` or similar);
    the frontend renders these files with a simpler keep-this-version/keep-that-version choice
    instead of the segment view (see Frontend section).
- `resolve_conflict` writes `resolved_content` verbatim to the file's working-tree path, then
  stages it (`Index::add_path` + `Index::write`) — this is what clears that path's conflicted
  state in the index. The frontend is responsible for reconstructing `resolved_content` from the
  segments it already holds plus the user's per-conflict choice; the backend does no resolution
  logic of its own, matching the project's existing "frontend owns display/assembly, backend does
  I/O" split (e.g. `DiffPane` already assembles hunks into a view; the backend never does).
- `abort_merge` calls `repo.cleanup_state()` (clears `MERGE_HEAD`/`MERGE_MSG`) then resets the
  working tree and index back to `HEAD` (`repo.reset(&head_commit, ResetType::Hard, None)`).
- `merge_message`/`is_merging` read `repo.message()` (git's own auto-generated message, e.g.
  `"Merge branch 'feature' into main"`, written by `repo.merge()`) and `repo.state()` — both
  read-only, exposed so the frontend can detect an in-progress merge and pre-fill the commit
  message.
- None of these take `&mut Repository` — `Index`/`Reference` mutation through `&Repository` is
  the same pattern `branch.rs`'s `switch_branch`/`create_branch` already use; only `stash.rs`
  needed `&mut Repository`, for a reason specific to stash's own git2 API shape.

### `commit.rs`: merge-aware parent selection

`commit()` currently always parents on `HEAD` alone. It's extended to also check
`repo.state() == RepositoryState::Merge` (or, more directly, `repo.mergehead_foreach`) and, if
so, include the `MERGE_HEAD` commit(s) as additional parents — exactly matching real `git
commit`'s own behavior after a merge. After a successful commit made while merging, `commit()`
calls `repo.cleanup_state()` to clear `MERGE_HEAD`/`MERGE_MSG`, the same cleanup `git commit`
itself performs. This is the one existing module this feature modifies rather than only adding
to — a merge commit is deliberately *not* a separate "finish merge" concept, it's a normal commit
that happens to have an extra parent, so the extension point is the existing `commit()`, not a
new function.

If the index still has conflicts when `commit()` runs, `Index::write_tree()` itself fails (git2
refuses to write a tree from an unmerged index) — surfaced as the commit's existing error path,
no new plumbing needed. This is also what backs the frontend's disabled-Commit-button behavior
described below; the UI gate is a UX nicety, not the only enforcement.

### `status.rs`: a new `Conflicted` status kind

`StatusKind` gains a `Conflicted` variant, detected via `git2::Status::CONFLICTED` (checked ahead
of the existing staged/unstaged flag checks — a conflicted path doesn't fit either bucket). A
conflicted file appears once in the status list with `kind: Conflicted`, `staged: false`.

### `tauri-app`: `Worker`/`Command` and Tauri commands

Five new operations, following the exact existing pattern per operation: `Command::StartMerge {
branch_name, reply }`, `GetConflictHunks { path, reply }`, `ResolveConflict { path,
resolved_content, reply }`, `AbortMerge { reply }`, `GetMergeMessage { reply }` → matching
`WorkerHandle` methods → `#[tauri::command]` pass-throughs in `commands.rs` (with
`MergeOutcomeDto`/`ConflictSegmentDto`, camelCase-serialized) → registered in `main.rs`.
`is_merging` doesn't need its own command — `merge_message`'s `Option<String>` already tells the
frontend whether a merge is in progress (`Some` ⇒ merging), one round-trip instead of two.

### `RepoClient` / frontend IPC

```ts
export type MergeOutcome =
  | { kind: "UpToDate" }
  | { kind: "FastForwarded" }
  | { kind: "Merged" }
  | { kind: "Conflicted"; files: string[] };

export type ConflictSegment =
  | { kind: "Clean"; content: string }
  | { kind: "Conflict"; ours: string; theirs: string };

mergeBranch(branchName: string): Promise<MergeOutcome>;
getConflictHunks(path: string): Promise<ConflictSegment[]>;
resolveConflict(path: string, resolvedContent: string): Promise<void>;
abortMerge(): Promise<void>;
getMergeMessage(): Promise<string | null>;
```

### Frontend state and components

`useAppState.ts`'s `AppState` gains `mergeMessage: string | null`, fetched via
`client.getMergeMessage()` in the same `Promise.all` `refresh()` already uses for
status/commits/branches/stashes. `null` means no merge is in progress.

**`BranchSwitcher.tsx`**: each branch row that isn't the current branch gains a "Merge into
current branch" action, in the same row as the existing Switch/Rename/Delete actions. Clicking it
calls `onMergeBranch(name)` (a new required prop, `(name: string) => Promise<void>`), which calls
`client.mergeBranch(name)` and triggers a `refresh()` — the resulting status list (now possibly
containing `Conflicted` entries) and `mergeMessage` drive everything downstream; there's no
separate "merge result" dialog.

Conflicted files render in the existing Uncommitted Changes list with a `Conflicted`-specific
label (mirroring how `New`/`Modified`/etc. already render distinctly). Clicking one selects it as
usual, but `DiffPane` renders a new **`ConflictResolutionPane`** in place of the normal
`UncommittedDiffPane`/`DiffView` for that file, instead of a diff — a conflicted file has no
single "diff" to show, it has two sides to reconcile.

**`ConflictResolutionPane.tsx`** (new component): on mount, calls `client.getConflictHunks(path)`
(same `useEffect` + `ignore`-guard fetch pattern every other async-loading pane in this codebase
already uses). Renders the returned segments top to bottom: `Clean` segments as plain preformatted
text, `Conflict` segments as a block showing both sides with "Accept Ours" / "Accept Theirs" /
"Accept Both" buttons — the user's choice is local component state, not sent to the backend until
resolution is saved. A "Save resolution" button reconstructs the final file text (joining `Clean`
segments verbatim and substituting each `Conflict` segment with its chosen side, or both
concatenated) and calls `client.resolveConflict(path, finalText)`, then triggers `refresh()` — a
resolved file drops out of the `Conflicted` bucket in the next status list. "Accept Both"
concatenates `ours` then `theirs`, in that order, joined by a newline if `ours` doesn't already
end in one — matching git's own `diff3`/`merge=union` convention of keeping "our" side first. If
`getConflictHunks` rejects (the add/delete-conflict case from the Architecture section), the pane
instead shows a simpler "keep our version / keep their version / delete file" choice, without
attempting to render a segment view for content that was never comparable line-by-line.

**`CommitBox.tsx`**: gains an optional `initialMessage?: string` prop, synced into its existing
internal `message` state via a `useEffect` that fires when `initialMessage` transitions from
`undefined`/`null` to a real string (so a merge's auto-generated message pre-fills the box the
moment a merge starts, without fighting the user's own edits afterward — the effect only seeds
the field once per merge, it doesn't keep overwriting what the user types). `App.tsx` passes
`appState.state.mergeMessage ?? undefined`. The existing Commit button's `disabled` condition
gains one more term: disabled while any status entry has `kind === "Conflicted"`. An "Abort
merge" button, visible only when `mergeMessage !== null`, calls a new `onAbortMerge` prop →
`client.abortMerge()` → `refresh()`.

### Error handling

No new plumbing: merge errors flow through the same `Result<T, String>` → rejected promise →
`state.error` → inline banner path every other feature already uses.

### Testing

- `crates/git-core/tests/merge.rs`: a fast-forwardable merge (ref moves, no merge commit
  possible/needed to inspect since none is created), an up-to-date merge (`UpToDate`, no
  changes), a clean divergent merge with no conflicts (`Merged`, working tree reflects both
  sides), a genuinely conflicting merge (same line edited on both branches → `Conflicted` with the
  right file list), `conflict_hunks` on that conflict returning the expected `Clean`/`Conflict`
  segment sequence, `resolve_conflict` clearing the conflict and letting a subsequent commit
  succeed with two parents, and `abort_merge` restoring the pre-merge working tree/index exactly.
- `crates/tauri-app/src/worker.rs`: thin wiring round-trip tests for each new command, matching
  the economical scope every other feature's worker tests already use.
- `frontend/src/components/ConflictResolutionPane.test.tsx` (new): renders segments correctly,
  Accept Ours/Theirs/Both updates local state and the eventual save call's reconstructed text,
  Save calls `resolveConflict` with the right final content.
- `frontend/src/components/CommitBox.test.tsx` additions: `initialMessage` pre-fills the field
  once and doesn't clobber subsequent user edits; Commit is disabled while a `Conflicted` entry
  exists in status; Abort merge button only renders when a merge is in progress and calls
  `onAbortMerge`.
- `frontend/src/components/BranchSwitcher.test.tsx` additions: clicking "Merge into current
  branch" calls `onMergeBranch` with the right branch name.
- One new E2E flow (`e2e/specs/`): create two branches that edit the same line differently, merge
  one into the other via the UI, see the conflicted file, resolve it (accept theirs on the
  conflicting hunk), commit, and see the resulting merge commit in the commit graph with two
  parents' worth of history reachable from it.
