# Sublime Merge parity, batch 3b: undo stack

## Goal

Make local Git operations reversible: Ctrl/Cmd+Z undoes the last operation in the active repo,
Ctrl/Cmd+Shift+Z redoes it. Covers ref-moving operations (commit, amend, reset, branch/tag
changes, checkout, cherry-pick, revert, merge, rebase, pull), index operations (stage/unstage
file, hunk, lines) and discards (file, hunk, lines, all unstaged). Discards stop being one-way
(batch 2a shipped them as hard discards "until the undo stack lands"). Both frontends (Tauri app,
VSCode extension) through `RepoClient`.

Batch 3 is split: 3a clone + init (shipped, #80), 3b undo stack (this spec), 3c commit signing.

## Current state

- `git-core/reflog.rs`: `list_reflog_refs`, `read_reflog`, `restore_reflog_entry` (refs only).
- `repo-service/worker/mod.rs`: one worker thread per open repo; every mutating command is a
  `Command` variant dispatched in a single `match` that hands the `reply` sender to a handler.
  Handlers send their own reply; the worker loop never sees success or failure.
- Merge and rebase can stop on conflicts (`StartMerge`, `StartRebase` / `RebaseContinue`,
  `AbortMerge`, `AbortRebase`; `rebase_state` lives in the worker loop).
- Discards (`DiscardHunk`, `DiscardLines`, `DiscardFile`, `DiscardAllUnstaged`) are permanent and
  sit behind a confirm dialog.
- Frontend: shortcut registry `lib/shortcuts.ts`, command palette `lib/commands.ts`, a single
  app-level toast region (3a), optimistic UI / busy guard around mutations.

## Decisions

- **Scope C**: refs + index + discards are undoable.
- **Generic before/after snapshots**, not per-op inverses. One restore mechanism serves undo and
  redo; new operations only need a label and a scope.
- **Persisted per worktree** in `<gitdir>/browsitory/undo.json`, capped at 50 undo entries, with
  objects kept alive by a hidden ref.
- **Redo** included; any new recorded operation clears the redo stack.
- **Strict drift check**: undo/redo is refused when the repo no longer matches the state the
  entry expects. No force option; the reflog stays the escape hatch.
- **UI**: shortcuts, toolbar buttons, command palette entries, toasts with an inverse action.
  No history panel.
- Not undoable: fetch, push, stash ops, submodule ops, worktree create/remove, remote config,
  credentials, clone/init.

## Backend (`git-core`)

### New `undo.rs`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadState {
    Branch(String),   // full ref name, e.g. "refs/heads/main" (may be unborn)
    Detached(String), // commit oid
}

// git-core stays serde-free; repo-service persists a mirror type (`StoredSnapshot`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoSnapshot {
    pub head: HeadState,
    pub refs: BTreeMap<String, String>,  // refs/heads/* and refs/tags/* -> oid
    pub index_tree: String,              // tree oid of the index
    pub worktree_tree: Option<String>,   // None for SnapshotScope::RefsAndIndex
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotScope { RefsAndIndex, Full }

#[derive(Debug, Error)]
pub enum UndoError {
    #[error("the repository changed since this operation")]
    RepositoryChanged,
    #[error("a merge, rebase, cherry-pick or revert is in progress")]
    OperationInProgress,
    #[error("the index has conflicts")]
    ConflictedIndex,
    #[error("git operation failed: {0}")]
    Git(#[from] git2::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn capture(repo: &Repository, scope: SnapshotScope) -> Result<RepoSnapshot, UndoError>;
pub fn restore(
    repo: &Repository,
    expected: &RepoSnapshot,
    target: &RepoSnapshot,
    reflog_message: &str,
) -> Result<(), UndoError>;
```

`capture`:
- `head`: symbolic target of HEAD (works for unborn branches), else detached oid.
- `refs`: every `refs/heads/*` and `refs/tags/*` (tag refs store the ref target, annotated tags
  included as the tag object oid).
- `index_tree`: `index.write_tree_to(repo)`; a conflicted index returns `ConflictedIndex`.
- `worktree_tree` (`Full` only): start an in-memory `git2::Index` from the HEAD tree (empty on
  unborn HEAD), overlay every path that status reports as modified, deleted, type-changed or
  untracked (not ignored; untracked directories recursed), hashing file bytes **raw** (no clean
  filters) with the on-disk mode (regular, executable, symlink), then `write_tree_to`. Deleted
  paths are removed from the overlay index.

`restore(expected, target)` only looks at the **components that differ** between `expected`
and `target` (HEAD if different, each ref whose value differs, the index if its tree differs, and
the worktree paths that differ between the two worktree trees). Unrelated changes elsewhere in
the repo (another branch moved, an untouched file edited) never block an undo. In order:
1. Refuse with `OperationInProgress` if `repo.state()` is not `Clean`.
2. **Verify** each differing component currently equals `expected` (a ref or file that must be
   absent must be absent; files compared by raw content hash, symlink-ness and — with
   `core.filemode` on — the executable bit). Any mismatch: `RepositoryChanged`, nothing written.
3. **Worktree** (both trees present): delete paths absent in `target` (removing directories left
   empty), then write each remaining differing path raw with its mode (symlinks as symlinks on
   Unix with `core.symlinks` on, else a plain file containing the link target).
4. **Index**: `read_tree(target.index_tree)` and `write`.
5. **Refs**: create/update/delete each differing ref, with `reflog_message`.
6. **HEAD**: `set_head` (symbolic) or `set_head_detached`.
Worktree goes first because it is the step most likely to fail (permissions, locked files); a
failure there leaves refs and HEAD untouched.

Undo calls `restore(after, before, "browsitory: undo <label>")`; redo calls
`restore(before, after, "browsitory: redo <label>")`.

## `repo-service`

### New `worker/undo.rs`: `UndoRecorder`

Owned by the worker thread, loaded on spawn.

```rust
pub struct UndoEntry { pub label: String, pub before: RepoSnapshot, pub after: RepoSnapshot }

impl UndoRecorder {
    fn begin(&mut self, repo: &Repository, label: String, scope: SnapshotScope);
    fn finish(&mut self, repo: &Repository);
    fn finalize_pending(&mut self, repo: &Repository); // merge/rebase completed
    fn drop_pending(&mut self);                        // merge/rebase aborted
    fn state(&mut self) -> UndoState;                  // { undo: Option<String>, redo: Option<String> }
    fn undo(&mut self, repo: &Repository) -> Result<String, UndoServiceError>;
    fn redo(&mut self, repo: &Repository) -> Result<String, UndoServiceError>;
}
```

- The worker loop calls `begin` before and `finish` after dispatching a recordable command.
  Handlers are unchanged.
- `finish` captures `after` with the same scope. `before == after` (failed or no-op command):
  nothing recorded. Otherwise push `{label, before, after}`, clear redo, trim to 50, persist.
- If `finish` sees `repo.state()` is a merge/rebase in progress, `before` and the label are kept
  as **pending** instead. `Commit` while merging and `RebaseContinue` that reaches done call
  `finalize_pending` (records `{label, before, after}` under the original label, e.g.
  "Merge feature into main"). `AbortMerge` / `AbortRebase` call `drop_pending`. Pending is
  in-memory only; an app restart mid-merge loses it (the merge itself is still abortable).
- `begin` capture failure (e.g. `ConflictedIndex`): log a warning (`eprintln!`, as nothing in
  repo-service logs otherwise), the command runs unrecorded.
- `begin` is a no-op while an operation is in progress (pending exists, or a merge/rebase was
  already running when the command arrived). "In progress" = `repo.state() != Clean` or the
  worker's `rebase_state.is_some()` (browsitory's rebase leaves `repo.state()` clean).
- Pending finalization is generic: after any command, if pending exists and nothing is in
  progress any more, capture `after`; `after == before` (aborted) drops it, otherwise it is
  recorded.
- `undo`/`redo` reload `undo.json` first if its mtime changed (another process — Tauri app and
  VSCode on the same repo), then call `git_core::undo::restore`, then move the entry between
  stacks and persist. Errors leave both stacks unchanged.

### Recordable commands

| Scope | Commands |
|---|---|
| `RefsAndIndex` | StageFile, UnstageFile, StageHunk, UnstageHunk, StageLines, UnstageLines, Commit, AmendCommit, CreateBranch, DeleteBranch, RenameBranch, CreateTag, DeleteTag |
| `Full` | DiscardHunk, DiscardLines, DiscardFile, DiscardAllUnstaged, IgnorePath, SwitchBranch, CheckoutCommit, ResetToCommit, CherryPick, RevertCommit, StartMerge, StartRebase, PullCurrentUpstream, RestoreReflogEntry |

Everything else is not recorded. Non-recorded commands that change local state (stash apply,
submodule update, …) make older entries fail the strict check, which is the intended outcome.

### Labels

Built in Rust at `begin` from the command's arguments; max 80 chars, paths middle-ellipsized,
commit summaries truncated with "…".

| Command | Label |
|---|---|
| StageFile / UnstageFile | Stage ⟨path⟩ / Unstage ⟨path⟩ |
| StageHunk / UnstageHunk / DiscardHunk | Stage hunk in ⟨path⟩ / Unstage hunk in ⟨path⟩ / Discard hunk in ⟨path⟩ |
| StageLines / UnstageLines / DiscardLines | Stage lines in ⟨path⟩ / Unstage lines in ⟨path⟩ / Discard lines in ⟨path⟩ |
| DiscardFile / DiscardAllUnstaged | Discard changes to ⟨path⟩ / Discard all unstaged changes |
| IgnorePath | Ignore ⟨path⟩ |
| Commit / AmendCommit | Commit “⟨summary⟩” / Amend “⟨summary⟩” |
| CreateBranch / DeleteBranch / RenameBranch | Create branch ⟨name⟩ / Delete branch ⟨name⟩ / Rename branch ⟨old⟩ to ⟨new⟩ |
| CreateTag / DeleteTag | Create tag ⟨name⟩ / Delete tag ⟨name⟩ |
| SwitchBranch / CheckoutCommit | Switch to ⟨branch⟩ / Check out ⟨short-oid⟩ |
| ResetToCommit | Reset to ⟨short-oid⟩ (⟨soft\|mixed\|hard⟩) |
| CherryPick / RevertCommit | Cherry-pick ⟨short-oid⟩ / Revert ⟨short-oid⟩ |
| StartMerge / StartRebase | Merge ⟨branch⟩ / Rebase onto ⟨target⟩ |
| PullCurrentUpstream / RestoreReflogEntry | Pull / Restore ⟨reference⟩ to ⟨short-oid⟩ |

### Storage

- `<gitdir>/browsitory/undo.json` (`repo.path()`, so each linked worktree has its own):
  `{ "version": 1, "undo": [UndoEntry…], "redo": [UndoEntry…] }`, oldest first. Written
  atomically (temp file + rename). Missing: empty stacks. Unparseable or unknown version: empty
  stacks, warning logged, file overwritten on next write.
- **GC protection**: ref `refs/browsitory/undo/<worktree-name>` (`main` for the main worktree)
  points at an anchor commit rewritten on every persist. Its tree has one subtree per snapshot
  tree (`<n>-before-index`, `<n>-before-worktree`, …); its parents are every distinct commit oid
  referenced by any snapshot's HEAD or refs (deleted branches stay reachable). Annotated tag
  objects: see the decision note below.
- The anchor ref and tag refs are deleted when both stacks are empty.

**Decision note — annotated tags:** a commit cannot reference a tag object, so a deleted
annotated tag's object would be unprotected. Store it instead as a ref
`refs/browsitory/undo/<worktree-name>-tags/<n>` per referenced tag object, rewritten with the
anchor. Lightweight tags need nothing (their commit is an anchor parent).

### Worker API

`WorkerHandle::get_undo_state() -> Result<UndoState, String>`,
`undo() -> Result<UndoOutcome, String>`, `redo() -> Result<UndoOutcome, String>`.

```rust
pub enum UndoOutcome {
    Done { label: String },
    Nothing,
    RepositoryChanged { label: String },
    OperationInProgress,
}
```

Refusals are ordinary outcomes, not errors, so they cross both transports unchanged (the VSCode
JSON-RPC error channel only carries a message string). `Err` is reserved for real failures (I/O,
libgit2); its message never includes file contents.

## IPC

- Tauri commands `get_undo_state`, `undo_last`, `redo_last` (`undo`/`redo` avoided as command
  names to keep them greppable); `vscode-sidecar` methods with the same names and payloads.
- `UndoOutcomeDto` serializes as `{ "kind": "Done", "label": … }` / `{ "kind": "Nothing" }` /
  `{ "kind": "RepositoryChanged", "label": … }` / `{ "kind": "OperationInProgress" }`.
- `RepoClient`: `getUndoState(repoPath): Promise<UndoState>`,
  `undo(repoPath): Promise<UndoOutcome>`, `redo(repoPath): Promise<UndoOutcome>`;
  `UndoState = { undo: string | null; redo: string | null }`.

## Frontend

- `AppState.undoState` is fetched in `useAppState`'s `refresh()` alongside status (so it updates
  after every mutation) and re-fetched alone on window `focus` (external changes).
- `useUndoActions` (composed into `useAppState` like the other action hooks) exposes
  `undo()` / `redo()` returning the outcome; `RepoWorkspace` turns `Done` into a toast.
- Shortcuts in `lib/shortcuts.ts`: `Mod+Z` undo, `Mod+Shift+Z` redo, plus `Ctrl+Y` redo on
  Windows/Linux. Ignored when focus is in `input`, `textarea` or a contenteditable element, so
  text fields keep native undo.
- Toolbar: undo/redo icon buttons. Tooltips "Undo ⟨label⟩" / "Redo ⟨label⟩", "Nothing to undo" /
  "Nothing to redo" when disabled. Disabled while any mutation is in flight.
- Command palette: "Undo ⟨label⟩" / "Redo ⟨label⟩", only listed when available.
- Undo and redo go through the existing busy guard (serialized with other mutations). On success:
  full repo refresh (status, graph, branches, open diff) and a toast.

Copy (exact):

| Event | Toast |
|---|---|
| Undo ok | "Undid ⟨label⟩" with action **Redo** |
| Redo ok | "Redid ⟨label⟩" with action **Undo** |
| `Nothing` | no toast |

Refusals and failures use the existing error banner (`state.error`), not a toast — toasts are
success-styled:

| Event | Error banner |
|---|---|
| `RepositoryChanged` | "Can't undo ⟨label⟩: the repository changed since. Use the reflog to recover." (redo: "Can't redo ⟨label⟩: …") |
| `OperationInProgress` | "Finish or abort the merge or rebase first." |
| rejection | "Undo failed: ⟨message⟩" / "Redo failed: ⟨message⟩" |

Toasts gain an optional action button (`Toast.action?: { label, run }`); clicking it runs the
action and dismisses the toast.

## Edge cases

- **Unrelated dirty files**: undo allowed; verification only covers HEAD, refs, the index and
  the paths the entry touches. A changed index (e.g. user staged after a commit) blocks undoing
  that commit — accepted strictness.
- **Large untracked files**: hashed into the object DB on every `Full` command. No size cap in
  this batch; documented in `docs/ARCHITECTURE.md` as a known cost.
- **CRLF / filters**: worktree blobs are raw bytes, restored raw — byte-exact round trip.
- **Modes / symlinks**: executable bit and symlinks preserved; `core.symlinks=false` restores the
  link as a plain file containing the target (git's behaviour).
- **Unborn HEAD**: HEAD tree is empty; undoing the first commit returns HEAD to unborn.
- **`git log --all` / gitk** show the anchor commit ("browsitory undo anchor") because it sits
  under `refs/browsitory/`. Accepted; documented in the user guide.
- **VSCode webview Ctrl/Cmd+Z**: the workbench may claim the key before the webview sees it. The
  toolbar buttons and palette are the host-independent paths (same stance as Ctrl/Cmd+W).
- **Two processes on one repo** (Tauri + VSCode): mtime reload before undo/redo; last writer
  wins on the file; the strict check protects repo state.

## Testing

- `git-core` `tests/undo.rs` (real temp repos): capture/restore round trip and redo for stage
  lines, discard file / hunk / untracked file, commit, amend, reset hard, delete branch, delete
  annotated tag, checkout commit, cherry-pick; refusals for moved ref, changed index, edited
  touched file, in-progress merge; unborn HEAD; symlink and executable bit (Unix-only tests
  `#[cfg(unix)]`); CRLF file round trip.
- `repo-service`: no entry for a failed/no-op command; redo cleared by a new command; pending
  merge finalized by `Commit` and dropped by `AbortMerge`; pending rebase finalized by
  `RebaseContinue`; persistence reload across worker restart; 50-cap trim; corrupt file ignored;
  anchor keeps a deleted branch's commit (and annotated tag object) alive across
  `git gc --prune=now` run through the `git` CLI (test skipped when `git` is not on PATH).
- `tauri-app`: `UndoState` / error DTO serialization only.
- Frontend (mock `RepoClient`): `useUndo` refresh triggers, shortcut input-focus guard, toolbar
  tooltips and disabled state, palette entries, toasts with inverse action, each error kind.
- E2E Tauri: discard a file, `Mod+Z` restores it, `Mod+Shift+Z` discards it again. E2E VSCode:
  commit, undo, file is staged again.

## Docs

- `CHANGELOG.md` entry; `docs/USER_GUIDE.md` "Undo and redo" section.
- `docs/ARCHITECTURE.md`: "Undo stack" section (snapshot model, storage, GC anchor, strict check,
  known cost).

## Out of scope

- Undoing fetch, push, stash, submodule, worktree, remote-config, credential, clone/init
  operations.
- History panel / multi-step jump.
- Forced undo past a drift check.
- Size cap for snapshotted untracked files.
