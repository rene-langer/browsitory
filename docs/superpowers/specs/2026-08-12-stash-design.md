# Stash Design

Status: Approved

## Context

Branch management (see `docs/superpowers/specs/2026-08-12-branch-management-design.md`) shipped
first of Phase 2's six subsystems. This spec covers the second: stash. Phase 2's remaining four
(merge, rebase, blame, commit graph) are out of scope here, each its own future spec.

Stash also closes a gap branch management deliberately left open: switching branches with a
dirty working tree blocks with a plain error (git2's safe checkout), and that design explicitly
chose not to integrate stash. Stash now exists as an independent feature the user can reach for
manually before switching — this spec does not wire any automatic "stash and switch" suggestion
into `BranchSwitcher`; that coupling is deferred, not assumed.

**Goals:**
- Save the current dirty state (tracked changes + untracked files) as a new stash.
- List existing stashes.
- Apply a stash (restore its changes into the working tree, stash entry remains).
- Drop a stash (remove it from the list, no restore).
- View a stash's diff by reusing the existing commit-diff UI — a stash is a regular commit under
  the hood, and `git-core::diff::commit_diff`/`commit_files` are generic over any commit id, so no
  new diff code is needed.

**Non-goals (explicitly deferred):**
- `pop` (apply + drop in one step) — not part of this backend; trivially composable client-side
  later (`applyStash` then `dropStash`) if wanted, but not built now.
- Conflict-marker UI for a stash apply that would leave the working tree in a conflicted state —
  git2's default safe checkout refuses cleanly instead (see Architecture), so there is no partial
  state to render. Real conflict resolution is the future merge subsystem's scope.
- `git stash branch` (create a branch from a stash).
- Selective/partial stash (pathspec-scoped) — always stashes everything dirty.
- Any automatic coupling with branch switching's dirty-tree block.

## Architecture

### The `&mut Repository` wrinkle

Every existing `git-core` function takes `&git2::Repository`. git2's stash API — `stash_save`,
`stash_apply`, `stash_drop`, `stash_foreach` — requires `&mut git2::Repository` instead (libgit2
holds an internal stash-ref lock that git2-rs models as a mutable borrow). This is a real,
load-bearing deviation from the established convention, not a style choice, and it has one
concrete consequence: `crates/tauri-app/src/worker.rs`'s worker-thread binding
(`let repo = repo;`) becomes `let mut repo = repo;` so stash calls can borrow mutably in the same
match arms that call every other module's `&repo` functions. No other part of the threading model
changes — the worker still owns one `Repository` per thread, `WorkerHandle`'s send/reply pattern
is unaffected, and `&mut` never crosses the channel (only owned `Send` command/reply values do,
same as today).

### `git-core` addition: `stash.rs`

Same shape as other modules otherwise: a `thiserror` `StashError` enum, tested against real
temp-dir repos (`crates/git-core/tests/stash.rs`).

```rust
pub struct StashEntry {
    pub index: usize,
    pub message: String,
    pub commit_id: String,
}

pub fn save(repo: &mut Repository) -> Result<(), StashError>;
pub fn list(repo: &mut Repository) -> Result<Vec<StashEntry>, StashError>;
pub fn apply(repo: &mut Repository, index: usize) -> Result<(), StashError>;
pub fn drop(repo: &mut Repository, index: usize) -> Result<(), StashError>;
```

- `save`: `repo.stash_save2(&repo.signature()?, None, Some(StashFlags::INCLUDE_UNTRACKED))` —
  the `message` argument is `None`, letting libgit2 generate its own default message (mirrors
  plain `git stash`'s own default, e.g. `"WIP on main: <short-id> <summary>"`); untracked files
  are always included per the earlier design decision (no flag to opt out, matching git's `-u`
  behavior as this project's fixed default). No custom stasher-message input in this spec — see
  Non-goals. (`stash_save2` is used instead of `stash_save` specifically because the latter
  requires a non-null message; `stash_save2` is the only variant that accepts `None`.)
- `list`: `repo.stash_foreach(...)` collecting `(index, message, oid)` into `Vec<StashEntry>`;
  `stash_foreach` also requires `&mut self` despite being read-only — a libgit2 API constraint,
  not a design choice.
- `apply`: `repo.stash_apply(index, None)` — `None` options means libgit2's default (safe)
  checkout strategy applies, refusing to overwrite a conflicting file exactly like
  `switch_branch`'s `checkout_tree(..., None)` does. Same philosophy, same error-propagation
  shape (`StashError::Git` wrapping the underlying `git2::Error`).
- `drop`: `repo.stash_drop(index)`. Indices shift after a drop (git2's own behavior, not
  something this module works around) — the frontend always re-fetches the full stash list after
  any mutation (see below), so a stale index is never held across calls.

### `tauri-app`: `Worker`/`Command` and Tauri commands

Same four-part pattern as branch management: `Command` enum gains `ListStashes`, `SaveStash`,
`ApplyStash { index: usize }`, `DropStash { index: usize }`; matching `WorkerHandle` methods;
matching `#[tauri::command]` pass-throughs in `commands.rs` (`list_stashes`, `save_stash`,
`apply_stash`, `drop_stash`); registered in `main.rs`'s `generate_handler!`. The worker thread's
`repo` binding becomes `mut` (see above) — every other match arm's `&repo` calls are unaffected
by that change (a `&mut` binding can still be borrowed immutably).

No new Tauri commands are needed for viewing a stash's diff — a `StashEntry.commit_id` is passed
straight to the existing `get_commit_diff`/`get_commit_files` commands unchanged, since those
operate generically on any commit id, stash or not.

### `RepoClient` / frontend IPC

```ts
export interface StashEntry {
  index: number;
  message: string;
  commitId: string;
}

listStashes(): Promise<StashEntry[]>;
saveStash(): Promise<void>;
applyStash(index: number): Promise<void>;
dropStash(index: number): Promise<void>;
```

`tauriRepoClient.ts` implements each as a thin `invoke()` call, matching every existing method's
pattern.

### Frontend state and components

`useAppState.ts` gains `stashes: StashEntry[]`, fetched in the same `Promise.all` as
`getStatus()`/`getLog()`/`listBranches()` inside `refresh()`, plus three `runMutation`-wrapped
mutations: `saveStash()`, `applyStash(index)`, `dropStash(index)`.

`HistoryList.tsx` gains a stash section, inserted between the "Uncommitted Changes" row and the
commit log. Each stash row:
- Reuses `SelectedRow`'s existing `{ commitId: string }` shape unchanged — a stash's `commit_id`
  is diffed identically to a real commit's by the existing `CommitDiffPane`/`commit_diff`/
  `commit_files` machinery, so `DiffPane` and `SelectedRow` need no new variant and no code
  change at all for viewing a stash's contents.
- Shows its message, plus inline `Apply`/`Drop` buttons (same per-row-button pattern
  `BranchSwitcher` already established for rename/delete), each calling
  `event.stopPropagation()` so clicking them doesn't also select the row.
- Participates in keyboard navigation (`rows` array) exactly like a commit row.

`DiffPane.tsx`'s uncommitted-changes pane gains a "Stash" button next to the existing
Stage/Unstage/Commit controls, calling `onSaveStash` (wired to `appState.saveStash`).

### Error handling

No new plumbing: stash errors flow through the same `Result<T, String>` → rejected promise →
`state.error` → inline banner path every other feature already uses.

### Testing

- `crates/git-core/tests/stash.rs`: save (asserts both a tracked change and an untracked file are
  captured, and that the working tree is clean afterward), list (message and commit id populated,
  most-recent-first order), apply (restores the working tree; a separate test asserts apply is
  blocked — returns an error, working tree untouched — when the target file conflicts with an
  uncommitted local edit, mirroring `switch_branch_is_blocked_by_a_conflicting_dirty_file`), apply
  on an empty stash list returns an error, drop (removes the entry), and a test asserting index
  reuse after a drop (stash twice, drop index 0, confirm the second stash is now index 0 via
  `list`).
- `crates/tauri-app/src/worker.rs`: thin wiring round-trip tests for all four ops (one or two
  tests proving the `Command`/`WorkerHandle` plumbing works end-to-end against a real repo), same
  economical scope as branch management's worker tests — not re-proving `git-core`'s own logic.
- `frontend/src/components/HistoryList.test.tsx`: the new stash section renders, Apply/Drop
  buttons call the right callback with the right index, and clicking a button doesn't also
  trigger row selection (`stopPropagation` verified via a click-count assertion on the row's own
  `onSelectRow`).
- `frontend/src/state/useAppState.test.ts`: `refresh()` fetches stashes, and each of the three
  mutations calls the right client method and refetches.
- One new E2E flow (`e2e/specs/`): dirty a tracked file → save a stash → see it listed with a
  message → apply it → see the file's content restored.
