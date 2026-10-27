# Feature parity, batch 4d: compare

## Goal

Compare any two commits, a branch against the current branch (since they diverged), and a
commit against the working tree, read-only, in the diff pane. Both frontends (Tauri app, VSCode
extension) through `RepoClient`. Refines section 4d of
`docs/superpowers/specs/2026-10-27-parity-batch4-roadmap-design.md` (issue #93).

## Current state

- `git-core::diff` has `commit_diff(repo, commit_id, path, opts)`,
  `commit_rename_diff(repo, commit_id, old_path, new_path, opts)` and
  `commit_files(repo, commit_id)`. All three compare a commit with its first parent (or the
  empty tree) through the private `commit_and_parent_trees`. They return `Vec<DiffHunk>` /
  `Vec<String>`. There is **no `FileDiff` type** anywhere, although the roadmap's plumbing
  sketch names one. `commit_files` does no rename detection. A rename shows up as a delete plus
  an add. Only file history knows about renames (`FileHistoryEntry.old_path`), and only file
  history calls `commit_rename_diff`.
- Literal pathspecs: every per-file diff sets `pathspec(path).disable_pathspec_match(true)`.
  `apply_display_options` applies `DiffOptions { ignore_whitespace, context_lines }` (context
  capped at `MAX_CONTEXT_LINES = 20`).
- `DiffError` has one variant, `Diff(#[from] git2::Error)` ("failed to compute diff: …"). No
  code outside `diff.rs` matches on it.
- The worker runs every diff read through `Command::GetCommitDiff` / `GetCommitFiles` /
  `GetCommitRenameDiff` (handlers in `worker/status.rs`). Tauri exposes them from
  `commands/status.rs`, and the sidecar from `dispatch.rs`, each with its own `DiffHunkDto`
  and `DiffOptionsDto`.
- Frontend: `DiffPane` renders `UncommittedDiffPane` for `selectedRow === "uncommitted"`, else
  `CommitDiffPane` (`CommitHeader` + one `CommitFileSection` per path). `CommitFileSection` owns
  the per-file diff/blame toggle, the **Blame** button and the **History** button
  (`aria-label "History of <path>"`). It fetches through `getCommitDiff`, or through
  `getCommitRenameDiff` when `oldPath` is set.
- `CommitGraph` keeps a multi-selection (`CommitSelection`, Ctrl/Cmd/Shift-click). When two or
  more commits are selected, `multiSelectItems` (Interactive rebase…, Squash/Fixup/Drop N
  commits) backs both the right-click menu and the toolbar (`role="toolbar"`,
  `aria-label "Selected commits"`). `singleCommitItems(commitId)` is the one-commit menu. The
  private `rowsEqual(a, b)` compares `SelectedRow`s.
- `BranchTree.branchContextItems(branch)` builds the local-branch menu (Checkout, Rename, Merge
  into current branch, Push/Set upstream on the current branch, Isolate branch, Delete).
  `remoteBranchItems(remoteName, branchName)` builds the remote-tracking branch menu (Checkout,
  Set as upstream for current branch).
- `App.tsx` (`RepoWorkspace`) passes one `<DiffPane>` as the `right` side of the split. File
  history and search swap the *left* list, never the right pane.

## Decisions

- **Hunks, not `FileDiff`.** The roadmap's `getRangeDiff(...): Promise<FileDiff>` does not match
  the code. Every per-file diff returns `DiffHunk[]`, so the range and working-tree diffs do too.
- **File lists carry renames: `ChangedFile { path, old_path }`.** The roadmap's
  `getRangeFiles(...): Promise<string[]>` cannot express "renamed across the range", and the
  roadmap requires rename detection. The range and working-tree file lists run
  `find_similar` with `renames(true)` (git's default 50% similarity, no copy detection).
  A renamed file is listed once, under its new path, with `old_path` set. The per-file diff takes
  `old_path: Option<&str>` and, when it is set, diffs both paths with rename detection, exactly
  like `commit_rename_diff`. `commit_files` (single-commit view) is unchanged; it still shows a
  rename as a delete and an add.
- **Two-dot semantics for two commits.** "Compare these commits" is a plain tree-to-tree diff
  (`git diff A B`), not a merge-base diff (`git diff A...B`), as the roadmap says. The commits
  need not be related.
- **"older..newer" means graph order.** Of the two selected commits, the one further down the
  loaded graph (higher index in `commits`, which is `TOPOLOGICAL | TIME` order) is `from`, the
  other `to`. This needs no ancestry query and matches what the user sees. **Swap** flips them.
- **Branch compare is `merge-base(HEAD, branch)..branch`** (`git diff HEAD...branch`), resolved
  in the backend by `branch_compare_range(repo, ref_name) -> CompareRange { from_id, to_id }`.
  It takes a full ref name (`refs/heads/x` or `refs/remotes/o/x`). On a detached HEAD it
  compares with HEAD. The menu then reads "Compare with HEAD" instead of "Compare with current
  branch". Errors (shown verbatim):
  - unborn HEAD: `There is no commit checked out to compare with.`
  - unrelated histories: `<short ref> and HEAD have no common ancestor.`
  - missing ref: `No branch named "<short ref>".`
- **Branch compare is also on remote-tracking branches** ("what would merging origin/main
  bring?"), with the same label. Local branch menu: only for non-current branches. The current
  branch would always compare empty.
- **Working-tree compare is `git diff <commit>` plus untracked files.** It runs
  `diff_tree_to_workdir_with_index` with `include_untracked`, `recurse_untracked_dirs` and
  `show_untracked_content`, so it covers staged, unstaged and untracked changes. That matches
  the Uncommitted view, which also lists untracked files (`working_diff_options`). Ignored files
  are excluded. Renames are detected among tracked files only (libgit2 does not pair untracked
  files without `for_untracked`).
- **Only commit ids cross the wire for ranges.** `fromId`/`toId`/`commitId` are 40-hex oids,
  parsed with `Oid::from_str` like `commit_diff`. Branch names are resolved once into ids by
  `getBranchCompareRange`. The frontend re-resolves on every `refreshGeneration`, so a branch
  that moved (fetch, commit) updates the compare.
- **The compare replaces the right pane; it does not add a `SelectedRow` kind.** `useCompare`
  holds `CompareTarget | null`. `App` renders `<CompareDiffPane>` instead of `<DiffPane>` while
  it is set. `SelectedRow` stays `"uncommitted" | { commitId }`, so the graph, search, file
  history and every `selectRow` caller are untouched. The commit draft and the discard request
  already live in `App`, so unmounting `DiffPane` loses nothing.
- **A compare closes when the selected row changes,** and when the user clicks × ("Close
  compare"). Clicking a commit afterwards shows that commit, as before. `useCompare` records
  the `selectedRow` it opened under. A different `selectedRow` drops the compare during that
  render (a render-time state adjustment, because `react-hooks/set-state-in-effect` forbids doing
  it in an effect), so it cannot come back later (the same rule as
  `CommitGraph`'s multi-selection: an outside focus change wins). Ctrl-clicking a third commit
  changes `selectedRow`, so it closes the compare as well.
- **Swap is only on two-commit compares.** A swapped branch compare (`branch..merge-base`) or a
  swapped working-tree compare (working tree → commit) does not answer a useful question.
- **Read-only; file history and blame from the file headers.** Each file section has the same
  **History** button as a commit's file list. **Blame** blames at `to` for commit and branch
  compares. A working-tree compare blames at `"HEAD"`, like the Uncommitted view's Blame
  button does today (`getBlame(repoPath, "HEAD", path)`). A file deleted in the range has no
  blame at `to`, and shows the existing "No blame available for this file at this revision."
  error.
- **One read-only file section for commits and compares.** `CommitFileSection` moves out of
  `DiffPane.tsx` into `components/ReadOnlyFileSection.tsx`. It now gets its diff through a
  `fetchDiff(opts)` callback, keyed by a `diffKey` string, and a `blameAt: string` revision.
  `CollapseToggle`/`CollapseAllToggle` move to `components/CollapseToggle.tsx`, so the new file
  does not import from `DiffPane.tsx`.
  `CommitDiffPane` and `CompareDiffPane` both use it, so the diff/blame toggle, collapse, and
  Retry/Dismiss behave the same in both.
- **Large ranges start collapsed.** Every expanded section fetches its own diff, and a range
  between distant commits can touch thousands of files. Above `COMPARE_EXPAND_LIMIT = 50` files,
  every section starts collapsed. The collapse-all toggle still works.
- **Working-tree compares follow the disk.** The file list and the open diffs reload on
  `refreshGeneration` (through the `diffKey`). Commit compares never reload, because commits are
  immutable. Branch compares reload because their resolved ids change.
- **No undo entry** (read-only; `undo::plan_for` already returns `None` for these commands), no
  palette command, no new shortcut. The roadmap lists none for 4d, and every entry point needs a
  row or a branch to act on.
- **No new dependency.** `docs/LICENSE_COMPLIANCE.md` does not change.

## Backend (`git-core/src/diff.rs`)

```rust
#[derive(Debug, Error)]
pub enum DiffError {
    #[error("failed to compute diff: {0}")]
    Diff(#[from] git2::Error),
    #[error("There is no commit checked out to compare with.")]
    UnbornHead,
    #[error("{0} and HEAD have no common ancestor.")]
    NoMergeBase(String),
    #[error("No branch named \"{0}\".")]
    UnknownRef(String),
}

/// One changed path in a compare. `old_path` is set when rename detection paired a delete
/// and an add; `path` is then the new name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedFile { pub path: String, pub old_path: Option<String> }

/// Resolved ends of a branch compare: the merge base with HEAD, and the branch tip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompareRange { pub from_id: String, pub to_id: String }

pub fn range_diff_files(repo: &Repository, from_id: &str, to_id: &str) -> Result<Vec<ChangedFile>, DiffError>;
pub fn range_diff(repo: &Repository, from_id: &str, to_id: &str, path: &str, old_path: Option<&str>, opts: &DiffOptions) -> Result<Vec<DiffHunk>, DiffError>;
pub fn compare_working_tree_files(repo: &Repository, commit_id: &str) -> Result<Vec<ChangedFile>, DiffError>;
pub fn compare_working_tree(repo: &Repository, commit_id: &str, path: &str, old_path: Option<&str>, opts: &DiffOptions) -> Result<Vec<DiffHunk>, DiffError>;
pub fn branch_compare_range(repo: &Repository, ref_name: &str) -> Result<CompareRange, DiffError>;
```

- A private `literal_path_options(path, old_path, opts)` builds the per-file `git2::DiffOptions`
  (literal pathspec for one or both paths, plus display options). `commit_diff` and
  `commit_rename_diff` switch to it, with no change in behavior. A private
  `detect_renames(&mut Diff)` and `changed_files(&Diff)` are shared by both file lists.
- The roadmap's `compare_working_tree(repo, commit, path, opts)` gains `old_path`, and gets a
  `compare_working_tree_files` sibling, for the same reason as the range functions.
- File lists come back in libgit2 delta order (sorted by path).

## `repo-service`, Tauri, sidecar, `RepoClient`

- `worker/compare.rs` (new): handlers and `WorkerHandle::{get_range_files, get_range_diff,
  get_working_tree_compare_files, get_working_tree_compare_diff, get_branch_compare_range}`.
  `Command::{GetRangeFiles, GetRangeDiff, GetWorkingTreeCompareFiles,
  GetWorkingTreeCompareDiff, GetBranchCompareRange}` in `mod.rs`.
- Tauri: `commands/compare.rs` (new). `ChangedFileDto { path, old_path }` and
  `CompareRangeDto { from_id, to_id }` (camelCase, `oldPath` serialized as `null`, not omitted).
- Sidecar: the same five methods, with its own DTO copies (convention).
- Wire methods: `get_range_files {repoPath, fromId, toId}`, `get_range_diff {repoPath, fromId,
  toId, path, oldPath, opts}`, `get_working_tree_compare_files {repoPath, commitId}`,
  `get_working_tree_compare_diff {repoPath, commitId, path, oldPath, opts}`,
  `get_branch_compare_range {repoPath, refName}`.
- `RepoClient`:

```ts
/** One changed path in a compare; `oldPath` is set when the file was renamed. */
export interface ChangedFile { path: string; oldPath: string | null; }
/** Resolved ends of a branch compare: merge base with HEAD → branch tip. */
export interface CompareRange { fromId: string; toId: string; }

getRangeFiles(repoPath: string, fromId: string, toId: string): Promise<ChangedFile[]>;
getRangeDiff(repoPath: string, fromId: string, toId: string, path: string, oldPath: string | null, opts: DiffOptions): Promise<DiffHunk[]>;
getWorkingTreeCompareFiles(repoPath: string, commitId: string): Promise<ChangedFile[]>;
getWorkingTreeCompareDiff(repoPath: string, commitId: string, path: string, oldPath: string | null, opts: DiffOptions): Promise<DiffHunk[]>;
getBranchCompareRange(repoPath: string, refName: string): Promise<CompareRange>;
```

  All of them go through `loggedInvoke` in `tauriRepoClient` (a failure here is unexpected, unlike
  search's per-keystroke errors).

## Frontend

- `state/useAppState.ts`: export `rowsEqual` (moved from `CommitGraph.tsx`).
- `state/useCompare.ts` (new):

```ts
export type CompareTarget =
  | { kind: "commits"; fromId: string; toId: string }
  | { kind: "branch"; refName: string; label: string }
  | { kind: "workingTree"; commitId: string };

export interface CompareState {
  target: CompareTarget;
  /** `toId === null` means the working tree. `null` while a branch compare resolves or failed. */
  range: { fromId: string; toId: string | null } | null;
  error: DescribedError | null;
}

export function useCompare(client: RepoClient, repoPath: string, refreshGeneration: number, selectedRow: SelectedRow): {
  compare: CompareState | null;
  compareCommits(fromId: string, toId: string): void;
  compareBranch(refName: string, label: string): void;
  compareWorkingTree(commitId: string): void;
  swap(): void;
  close(): void;
  retry(): void;
};
```

- `components/ReadOnlyFileSection.tsx` (moved from `DiffPane.tsx`'s `CommitFileSection`).
- `components/CompareDiffPane.tsx` (+ `.module.css`, new): a header `section`
  (`aria-label "Comparison"`), then the file sections.

| State | Heading (`h2`) | From / To (`dl`) |
|---|---|---|
| two commits | `Comparing <from7>..<to7>` | `<short> <summary>` each |
| branch | `Changes on <label> since it diverged` | `<short> (merge base)` / `<short> <label>` |
| working tree | `Comparing <from7> with the working tree` | `<short> <summary>` / `Working tree` |

  `<short>` is the commit's `shortId` (with its summary) when it is among `commits`, else the
  first 7 characters of the id. Buttons: **Swap** (`aria-label "Swap sides"`, two-commit only)
  and **×** (`aria-label "Close compare"`). Status line: `1 file changed`, `N files changed`, or
  `No differences.`. While a branch compare resolves: `Finding where <label> diverged…`. A
  failure (resolve or file list) renders `InlineError` with Retry and Dismiss (Dismiss closes
  the compare).
- `CommitGraph`: new optional props `onCompareCommits(fromId, toId)` and
  `onCompareWithWorkingTree(commitId)`. With exactly two selected commits, `multiSelectItems`
  gains **Compare these commits** as its last item, in both the menu and the toolbar. It is
  never disabled: it is read-only, so `pending` and a blocked rebase plan do not apply.
  `singleCommitItems` gains **Diff against working tree** (never disabled). Each item shows only
  when its prop is passed.
- `BranchTree`: new optional prop `onCompareBranch(refName, label)`. Non-current local branches
  get **Compare with current branch** (`refs/heads/<name>`, label `<name>`). Remote-tracking
  branches get the same item (`refs/remotes/<remote>/<branch>`, label `<remote>/<branch>`). It
  reads **Compare with HEAD** when no local branch is current.
- `App.tsx`: `useCompare(...)`. Pass the handlers to `CommitGraph`/`BranchTree`. The `right`
  side is `<CompareDiffPane>` while `compare.compare !== null`, else `<DiffPane>`.
  `CompareDiffPane` gets `diffPaneCommits` for the header lookups and `fileHistory.open` for
  History.

## Overlap with other batches (4b and 4c merge before 4d is implemented)

- **4b, branch menus.** 4b adds "Push to…" to the local branch menu, "Delete <remote>/<branch>…"
  to the remote branch menu, and `↑n ↓m` on rows. 4d adds "Compare with current branch" to both
  menus. Place it after "Merge into current branch" (local) and after "Set as upstream for
  current branch" (remote), keeping whatever 4b inserted. The BranchTree tests that list menu
  items must include both.
- **4b, `GraphCommit.tags`.** Every `GraphCommit` literal in this plan's tests needs the field
  4b adds (`tags: []`), and so does the sidecar/Tauri `GraphCommitDto` if a test builds one.
- **4c, read-only diff pane modes.** 4c's stash diff shows a stash in the diff pane (read-only,
  with an Untracked group). If 4c already split `CommitFileSection` into a reusable read-only
  section, reuse it and its name instead of creating `ReadOnlyFileSection`. If 4c added a
  stash view state, make the two mutually exclusive: opening a compare closes the stash view,
  and the other way round. Opening either one replaces the right pane.
- **4c, `FileDiff`.** If 4c introduced a `FileDiff`-like type (`path` + `old_path` + hunks) for
  `stash_diff`, check whether `ChangedFile` should reuse its path fields. Do not change 4c's type.
- **4c and 4b, `RepoClient` fixtures.** Both add methods, so every full `RepoClient` literal in
  the frontend tests changes in all three batches. Re-run `tsc` after the rebase.
- **E2E (#98–#100).** The new spec creates its own commits on unique files, calls
  `browser.refresh()` first, and asserts only on its own commits. It does not depend on the
  branch name, earlier specs, or a clean working tree.

## Testing

- `git-core` `tests/compare.rs` (real temp repos): non-adjacent range lists every path and
  shows the net change; reversed order swaps adds and removes; same commit is empty; rename
  across the range is one entry and its diff shows only the edit; a file deleted in the range;
  unknown commit id errors; working tree lists staged, unstaged and untracked changes and a
  deletion; working tree against an older commit includes later commits; branch compare uses
  the merge base and ignores HEAD-only commits; remote-tracking ref; detached HEAD; unrelated
  history; unknown ref; unborn HEAD.
- `repo-service`: range files + diff, working tree files + diff, and branch range round-trip
  through the worker.
- `tauri-app`: `ChangedFileDto` / `CompareRangeDto` serialization.
- `vscode-sidecar`: all five methods round-trip.
- Frontend (mock `RepoClient`): both clients' wiring; `useCompare` (open each kind, swap, close,
  closes on selection change and does not resurface, branch re-resolve on refresh, stale
  resolve dropped, retry); `ReadOnlyFileSection` (custom fetch keyed by `diffKey`, blame at
  `blameAt`); `CompareDiffPane` (headings, swap only for commits, counts, collapse over the
  limit, working-tree reload on refresh, error with Retry/Dismiss); `CommitGraph` (item only with
  exactly two, older/newer order, enabled while pending, working-tree item); `BranchTree`
  (local non-current only, remote, HEAD wording); `App` (two commits → compare → × restores
  the commit view).
- E2E Tauri (`e2e/specs/compare.spec.ts`): three commits on a unique file. Select the first and
  third, Compare these commits, see `Comparing <base7>..<tip7>` and the file. Swap shows
  `<tip7>..<base7>`. × returns to the commit view.

## Docs

- `docs/USER_GUIDE.md`: a new "Comparing commits and branches" section after "Searching commits".
- `CHANGELOG.md` under Unreleased: "Compare".
- No `docs/ARCHITECTURE.md` change (no new mechanism), and no `docs/LICENSE_COMPLIANCE.md` change.

## Out of scope

Comparing across repositories; three-way compare views; combined diffs of merges; compare from
search results or file history rows; tag compare from 4b's tag badges (a follow-up, using the
same `getBranchCompareRange` with `refs/tags/…`); copy detection; a palette entry.
