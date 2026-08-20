# Hunk-based staging + right-aligned row actions

Date: 2026-08-20

## Goal

Stage, unstage, and discard individual diff hunks from the diff view, instead
of only whole files. Bundled with a small UI rework: row action buttons
(branches, stashes, worktrees, tags, remotes, PRs, reflog, and the new
per-hunk buttons) right-align, Sublime Merge style, so rows read as
`label ......... [buttons]` instead of everything crowding the left edge.

## Non-goals

- Line-level (sub-hunk) staging. Hunk is the smallest unit.
- Whole-file discard. Doesn't exist today; out of scope here — hunk discard
  is scoped narrowly to hunks, not a general "add discard everywhere" pass.
- Changing the top-level action bars (Commit box, Stash/Push/Pull buttons) —
  only per-row action groups right-align.

## 1. IPC surface / backend

Three new operations, each keyed by `(repoPath, path, oldStart, newStart)` —
the hunk's `@@ -oldStart,oldLines +newStart,newLines @@` header pair is
unique within one diff, so no new id field is needed on `DiffHunk`.

`crates/git-core/src/stage.rs` gains:

```rust
pub fn stage_hunk(repo: &Repository, path: &str, old_start: u32, new_start: u32) -> Result<(), StageError>;
pub fn unstage_hunk(repo: &Repository, path: &str, old_start: u32, new_start: u32) -> Result<(), StageError>;
pub fn discard_hunk(repo: &Repository, path: &str, old_start: u32, new_start: u32) -> Result<(), StageError>;
```

Approach (mirrors `git add -p` / `git reset -p` / `git checkout -p`):

- **stage_hunk**: recompute the unstaged diff (`diff_index_to_workdir`,
  scoped to `path`), apply it to `ApplyLocation::Index` via
  `Repository::apply` with an `ApplyOptions::hunk_callback` that returns
  `true` only for the hunk whose header matches `(old_start, new_start)`.
- **unstage_hunk**: recompute the staged diff (`diff_tree_to_index`, HEAD vs
  index, scoped to `path`) with `DiffOptions::reverse(true)`, apply the
  matching hunk to `ApplyLocation::Index`.
- **discard_hunk**: recompute the unstaged diff with `reverse(true)`, apply
  the matching hunk to `ApplyLocation::WorkDir`.

Exact hunk-header matching after reversal (old/new swap roles when
`reverse(true)` is set) gets worked out via TDD during implementation, not
finalized here.

New Tauri commands (`stage_hunk`, `unstage_hunk`, `discard_hunk`) and
`RepoClient` methods, following the exact shape `stageFile`/`unstageFile`
already use:

```typescript
stageHunk(repoPath: string, path: string, oldStart: number, newStart: number): Promise<void>;
unstageHunk(repoPath: string, path: string, oldStart: number, newStart: number): Promise<void>;
discardHunk(repoPath: string, path: string, oldStart: number, newStart: number): Promise<void>;
```

## 2. Frontend components

`DiffView.tsx` gets three new optional props:

```typescript
onStageHunk?: (oldStart: number, newStart: number) => void;
onUnstageHunk?: (oldStart: number, newStart: number) => void;
onDiscardHunk?: (oldStart: number, newStart: number) => void;
```

A per-hunk `Toolbar` renders in the `hunkHeader` row, right-aligned, showing
whichever single one of Stage/Unstage is wired (never both) plus Discard
when wired. `CommitDiffPane` passes none of the three — historical diffs
stay read-only, unchanged.

`UncommittedDiffPane` wires `onStageHunk`/`onUnstageHunk` based on
`selected.staged`, and always wires `onDiscardHunk`. All three disabled via
the same `repositoryOperationDisabled` gate already threaded through
`DiffPane`.

Discard confirm: two-click "Discard" → "Confirm Discard" on the same button,
same pattern as `BranchSwitcher`'s force-delete — no modal, fits the
cramped per-hunk row.

## 3. Data flow / state wiring

`useAppState.ts` gains `stageHunk`/`unstageHunk`/`discardHunk`, each a
`runMutation(() => client.stageHunk(repoPath, path, oldStart, newStart))` —
identical shape to `stageFile`/`applyStash`. `runMutation` already refreshes
`status` after any mutation; `DiffPane`'s diff-fetch effect already depends
on `status`, so the hunk list re-fetches for free — the same mechanism that
already makes whole-file stage/unstage refresh the pane. No new plumbing
beyond the three action creators plus prop threading through
`App.tsx` → `DiffPane` → `DiffView`.

## 4. Error handling

A stale-hunk mismatch (file changed between fetch and click) surfaces as a
`StageError`/`git2::Error`, same as any other mutation — `runMutation`
already catches and routes it to `state.error`, no new error path. No
optimistic UI: buttons stay enabled until the mutation round-trips;
`repositoryOperationDisabled` blocks double-clicks meanwhile, same guard
stash Apply/Drop already uses.

## 5. Testing

- `git-core::stage`: new Rust tests per op — stage a single hunk from a
  multi-hunk file diff, verify index content plus that the remaining
  unstaged hunk is untouched; same for unstage/discard; a discard test
  asserts workdir content reverted.
- `DiffView.test.tsx`: buttons render only when the matching callback prop
  is passed; click calls back with `(oldStart, newStart)`; discard
  two-click confirm behavior.
- `DiffPane.test.tsx`: wiring — staged view shows Unstage+Discard, unstaged
  view shows Stage+Discard, commit-diff view shows neither.
- `useAppState.test.ts`: new actions follow the existing `runMutation` test
  pattern (call client method, refresh status).
- e2e: one spec exercising stage-hunk → commit → verify only that hunk's
  content landed (multi-hunk fixture file), matching
  `stash-management.spec.ts`'s style.

## 6. UI rework — sitewide right-align

`Toolbar.module.css`'s `.toolbar` changes to `justify-content: flex-end`.

Every row that puts a `Toolbar` next to a label (branch name, stash
message, worktree path, tag name, remote name, reflog entry, PR row, and
the new per-hunk header) needs the label to take the left and the toolbar
pushed right. Rows already using `ListRow` (its `.row` is flex) mostly get
this for free; plain `<li>` lists (`WorktreePanel`, `TagPanel`,
`RemotePanel`, `ReflogPanel`, `PullRequestPanel`) need a small per-file CSS
touch (`display: flex` on the row, `flex: 1` or similar on the label) to
make room. Audited and adjusted per-component during implementation — no
blind global flip that could break rows nobody checked.

Top-level action bars that aren't row actions (Commit box, Stash/Push/Pull
buttons) are unaffected — this only touches per-row action groups.
