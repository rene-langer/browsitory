# Feature parity, batch 2b: file history

## Goal

Show the commit history of one file, following renames, inside the commit graph area: pick a
file, the graph turns into a filtered list of the commits that changed it, and selecting a commit
shows that file's diff at that commit. Works in both frontends (Tauri app, VSCode extension)
through `RepoClient`.

Builds on batch 2a (`DiffOptions` on `commit_diff`). Out of scope: line history (`git log -L`),
copy detection, history over the graph's branch selection (HEAD only), directory history, commit
search (own plan).

## Current state

- `git-core/graph.rs`: `graph_log(repo, limit, selected_branches)` returns `GraphCommit`s (id,
  summary, author, timestamp, parents, local/remote branch refs). Frontend pages with
  `GRAPH_PAGE_SIZE` and "Load more" (`useAppState.ts` `graphLimit`).
- `git-core/diff.rs`: `commit_diff(repo, commit_id, path, opts)` diffs one literal path against
  the first parent, no rename detection. `commit_files` lists changed paths (a rename shows as
  delete + add).
- `DiffPane.tsx`: working-tree file header has Blame and a file menu (`FileActionsMenu.tsx`:
  discard, ignore); commit-file header has Blame.
- IPC wiring per method: `repo-service/src/worker/*`, `tauri-app/src/commands/*`,
  `vscode-sidecar/src/dispatch.rs`, `frontend/src/ipc/{RepoClient,tauriRepoClient,vscodeRepoClient}.ts`.

## Decisions

- **Walk with git2, no `git` subprocess.** Keeps the git2-only architecture.
- **History starts at HEAD**, like `git log --follow <path>`. Graph branch selection is ignored.
- **Inclusion = git's TREESAME rule.** A non-merge commit is shown when the blob (OID + mode) at
  the tracked path differs from its parent. A merge is shown only when it differs from *every*
  parent, so side-branch changes appear once, on the side-branch commit.
- **Renames followed per lineage**, not with a single global "current name": the tracked path is
  carried from child to parent, so a rename on one branch doesn't rename the path on another.
- **Rename detection**: libgit2 `find_similar` with renames only, default 50% threshold, no
  copies. Run only when the tracked path is missing in a parent.
- **Filtered graph mode, not a separate view**. Flat rows, no lanes (history
  parents aren't adjacent rows).
- **Pagination like the graph**: `limit` (default `GRAPH_PAGE_SIZE`), "Load more" re-walks with a
  larger limit.

## Backend (`git-core`)

### New `history.rs`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHistoryEntry {
    pub commit: GraphCommit,      // same struct/ref decoration as graph_log
    pub path: String,             // path of the file in this commit
    pub old_path: Option<String>, // Some(prev) when this commit renamed prev -> path
}

#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("Path must be relative to the repository root.")]
    InvalidPath,
    #[error(transparent)]
    Git(#[from] git2::Error),
}

pub fn file_history(repo: &Repository, path: &str, limit: usize)
    -> Result<Vec<FileHistoryEntry>, HistoryError>
```

Algorithm:

1. Reject empty, absolute, or `..`-containing paths ⇒ `InvalidPath`. Unborn HEAD ⇒ `Ok(vec![])`.
2. Revwalk from HEAD with `Sort::TOPOLOGICAL | Sort::TIME` (children before parents).
   `tracked: HashMap<Oid, String>` seeded with `HEAD → path`.
3. For each commit `c` with an entry in `tracked` (others are skipped; they can't reach the
   file's lineage from HEAD's path):
   - `entry_c = c.tree().get_path(p)` (missing ⇒ the file doesn't exist here on this lineage;
     skip, propagate nothing).
   - For each parent `q`: look up `p` in `q`'s tree. Same OID and mode ⇒ TREESAME to `q`. Missing
     ⇒ run `diff_tree_to_tree(q, c)` with `find_similar(renames)`; a `Renamed` delta whose new
     path is `p` gives the old path `o` for that parent; none ⇒ the file was added relative
     to `q`.
   - Include `c` if it is not TREESAME to any parent (root commit: included if the file exists).
     `old_path` = the rename source found against the first parent, if any.
   - Propagate to each parent the path to track there (`p`, or `o` after a rename; nothing after
     an add), unless the parent already has an entry (first child wins — topological order makes
     this the nearest descendant).
4. Stop after `limit` included commits.

Ref decoration: factor `graph.rs`'s tips-by-OID collection and `GraphCommit` construction into a
`pub(crate)` helper used by both `graph_log` and `file_history`.

### `diff.rs`

```rust
/// Diff of a file renamed by `commit_id` (old_path -> new_path), against the first parent,
/// with rename detection so content changes show as edits, not a whole-file add.
pub fn commit_rename_diff(repo, commit_id, old_path, new_path, opts: &DiffOptions)
    -> Result<Vec<DiffHunk>, DiffError>
```

Pathspec both literal paths, `find_similar(renames)`, then `hunks_from_diff`. A pure rename (100%
similar) returns no hunks.

## IPC

New `RepoClient` methods:

```ts
export interface FileHistoryEntry { commit: GraphCommit; path: string; oldPath: string | null; }

getFileHistory(repoPath: string, path: string, limit: number): Promise<FileHistoryEntry[]>;
getCommitRenameDiff(repoPath: string, commitId: string, oldPath: string, newPath: string,
                    opts: DiffOptions): Promise<DiffHunk[]>;
```

Wired through `repo-service` worker, Tauri commands, VSCode sidecar dispatch (camelCase DTO,
`oldPath` serialized as `null`, not omitted), both TS clients.

## Frontend

### Entry points and state

- Working-tree entry point is a **History button next to Blame** (hidden for untracked files),
  not a file-menu item: the file menu only exists for unstaged, non-conflicted rows, while
  history is useful for staged files too.
- File-history state lives in its own hook `frontend/src/state/useFileHistory.ts` (used by
  `RepoWorkspace` in `App.tsx`), not inside `useAppState` (already 21 KB and split into action
  hooks).
- Palette command is "File history of current file" and uses the existing
  `DiffCommands.currentFile` (the commit view has no single selected file).
- Opening history selects its newest entry once loaded.
- In history mode the commit diff pane shows only the history file, with a "Show all N files"
  button.

### Graph in history mode

- Banner above the rows: `History: <path>` and a × button. Esc (when focus is in the graph)
  also closes. Closing restores the commit selection held before opening.
- Rows: same row component and styling as the graph (summary, refs, author, date), lane column
  hidden. Rename rows show a "renamed from `<old_path>`" chip.
- "Load more" when `hasMore` (`entries.length >= limit`).
- Empty list: "No committed history for `<path>`."
- Error: message in the banner area with Retry.
- Keyboard up/down navigation as in the graph.

### Selecting a history row

Opens the normal commit view for that commit with the file at the entry's `path` preselected.
For entries with `oldPath`, the diff comes from `getCommitRenameDiff` and the file header shows
"Renamed from `<oldPath>`". Blame in this view blames `path` at that commit (existing behaviour).

## Error handling

- `HistoryError::InvalidPath` shown as its message; `Git` errors shown as-is, with Retry.
- Path not present at HEAD (e.g. staged new file opened from palette) ⇒ empty list, not an error.

## Testing

- `git-core` (real temp-dir repos, `tests/history.rs`):
  - linear edits: only commits touching the file, newest first;
  - commits not touching the file are skipped;
  - pure rename followed, `old_path` set; rename + edit followed;
  - rename on a side branch, then merge: both lineages followed with their own paths;
  - merge TREESAME rule: merge identical to one parent not listed; conflicting merge resolution
    listed;
  - file deleted then re-added: history stops at the re-add;
  - `limit` respected; unborn HEAD ⇒ empty; `..` path ⇒ `InvalidPath`;
  - `commit_rename_diff` shows only the edited lines; pure rename ⇒ no hunks.
- `tauri-app`: DTO serialization of `FileHistoryEntry` (`oldPath: null`).
- `vscode-sidecar`: dispatch round-trip for both methods.
- Frontend (Vitest, mocked `RepoClient`): each entry point opens history; banner, ×, Esc restore
  selection; rename chip and rename diff call; Load more; empty and error states; refresh keeps
  history open; stale response dropped.
- E2E (Tauri): open file history from the file menu, select an older commit, see its diff, close.
  VSCode E2E: open file history smoke test.

## Docs

- `docs/USER_GUIDE.md`: "File history" section (entry points, rename following, HEAD-only).
- `CHANGELOG.md` Unreleased: Added entry.
