# Feature parity, batch 4: roadmap

## Goal

Batches 1–3c closed the everyday-workflow gaps (amend, cherry-pick, revert, reset, force push,
detached checkout, line staging, diff options, discard/ignore, file history, clone/init, undo,
signing). This spec lists the functional gaps that remain against mature desktop Git clients,
confirmed against the code on `main` (f168012), and splits them into seven independently
shippable batches. Each batch gets its own `-design.md` refinement and `docs/superpowers/plans/`
plan before implementation, as batches 1–3c did.

## Gap inventory

Confirmed missing (file references are where the work lands):

| # | Gap | Evidence |
|---|---|---|
| 1 | Commit search (text, author, path, message, content) | No search command in `RepoClient.ts`; the commit-search plan referenced by batch 2a never landed |
| 2 | Tags as badges in the commit graph | `git-core/graph.rs` collects local/remote branch tips only |
| 3 | Delete a remote branch; push a non-current branch; publish a new branch with upstream | `branch.rs`/`remote.rs` have no such functions; `push_current_branch` pushes only the current branch |
| 4 | Fetch all remotes; prune stale remote-tracking branches | `fetch_remote` takes one remote, no prune |
| 5 | Merge options: no-ff, squash, ff-only | `start_merge` always fast-forwards when possible |
| 6 | Stash options: message, keep index, selected files, pop, view stash contents | `save_stash` always uses default message + `INCLUDE_UNTRACKED`; no diff of a stash |
| 7 | Compare two commits / branch vs HEAD | Multi-select only offers rebase actions |
| 8 | Git hooks (pre-commit, commit-msg, …) | libgit2 runs none; batch 1 listed hooks out of scope |
| 9 | External editor, file manager, terminal, diff/merge tool | Only `open_url` via `tauri_plugin_opener` |
| 10 | Whole-file "use ours / use theirs" for content conflicts | `ConflictResolutionPane` has per-segment choices only (whole-file only for add/delete) |
| 11 | Cherry-pick/revert several commits; merge commits with a mainline | `PickError::MergeCommitUnsupported`; single commit only |
| 12 | Image diff preview | Binary files show "No text differences" |
| 13 | Commit message guidance (summary length ruler), author override on amend, co-author trailers | `CommitBox` has no such controls |

Not planned (deliberately): Git LFS, bisect, sparse checkout, partial clone, x509 signing.

## Order and rationale

1. **4a – Commit search** (gap 1). Most-used missing feature on large histories.
2. **4b – Refs and remotes** (gaps 2–4). Small, high-frequency, all in `remote.rs`/`graph.rs`.
3. **4c – Merge and stash options** (gaps 5–6).
4. **4d – Compare** (gap 7). Reuses `commit_diff` plumbing.
5. **4e – Git hooks** (gap 8). Needs a process runner; same pattern as `signing.rs`.
6. **4f – External tools** (gap 9). Desktop-first, VSCode maps to native APIs.
7. **4g – Conflicts, picks and polish** (gaps 10–13).

Every batch: native `git2` where libgit2 supports it, `RepoClient` additions implemented in both
`tauriRepoClient` and `vscodeRepoClient`, `repo-service` worker ops, Tauri commands,
`vscode-sidecar` methods, DTO serialization tests in `tauri-app`, real temp-repo tests in
`git-core`, Vitest with a mocked `RepoClient`, one Tauri E2E flow per batch, user guide section
and `CHANGELOG.md` entry. Ref-moving operations record an undo entry (batch 3b's generic
snapshot) unless stated otherwise.

---

## 4a – Commit search

### Backend (`git-core/search.rs`, new)

- `pub struct SearchQuery { terms: Vec<SearchTerm> }` parsed by
  `parse_query(input: &str) -> Result<SearchQuery, SearchError>`. Terms are ANDed; quoted
  values allowed.
  - bare text → summary/message or SHA prefix
  - `author:` (name or email, case-insensitive substring), `committer:`
  - `message:` (full message), `commit:` (SHA prefix)
  - `path:` (commit touches a path; glob via `git2::Pathspec`)
  - `contents:` (an added or removed line contains the text — pickaxe-like, computed from the
    commit-vs-first-parent diff, limited to `path:` when given)
  - `after:` / `before:` (`YYYY-MM-DD`, committer date)
  - `branch:` (reachable from a ref; default: the graph's current branch selection)
- `search_commits(repo, query, refs, cursor: Option<String>, limit: usize, cancel: &dyn Fn() -> bool)
  -> Result<SearchPage, SearchError>` where `SearchPage { commits: Vec<GraphCommit>, next_cursor:
  Option<String>, scanned: usize }`. Revwalk in topological+time order, cheap filters first
  (SHA, author, dates, message), diff-based filters (`path:`, `contents:`) last. Returns once
  `limit` matches are found or 20,000 commits are scanned, so a page never blocks the worker
  for long; `next_cursor` resumes the walk.
- Errors: `InvalidQuery { message }` (unknown key, bad date), `Cancelled`.

### Plumbing

```ts
searchCommits(repoPath: string, query: string, refs: string[], cursor: string | null): Promise<SearchPage>;
cancelSearch(repoPath: string): Promise<void>;
```

The worker keeps one in-flight search per repo; a new search cancels the previous one (shared
cancel flag, like `cancel_transfer`).

### Frontend

- Search field above the commit graph (Ctrl/Cmd+F when the graph has focus, palette "Search
  commits"). Debounced 250 ms. Results replace the graph rows (no lanes, like file history) with
  match count, "Load more" from `next_cursor`, "Scanned N commits…" while incomplete.
- Query help popover listing the keys. Invalid query shows the backend message inline.
- Esc or × restores the graph and previous selection (same mechanism as file history).

### Tests

Parser unit tests (keys, quotes, dates, errors); real-repo tests per key; `contents:` finds an
added and a removed line; paging resumes without duplicates; cancel stops the walk. Frontend:
debounce, result list, load more, error, restore. E2E: search by author, open a result.

### Out of scope

Regex, `OR`/negation, searching stashes or reflog, persisted search history.

---

## 4b – Refs and remotes

### Backend

- `graph.rs`: `GraphCommit` gains `tags: Vec<String>` (lightweight and annotated, peeled to the
  commit). Collected in the same ref pass as branch badges.
- `remote.rs`:
  - `delete_remote_branch(repo, remote, branch, credentials, reporter, cancel)` pushes
    `:refs/heads/<branch>` and deletes `refs/remotes/<remote>/<branch>` on success.
    `TransferOperation::DeleteRemoteBranch`.
  - `push_branch(repo, remote, local_branch, remote_branch, set_upstream: bool, …)`
    generalizes `push_current_branch` (which becomes a wrapper). `set_upstream` writes
    `branch.<name>.remote` / `.merge` after a successful push.
  - `fetch_all(repo, prune: bool, …)` fetches every remote in sequence with one operation id;
    `fetch_remote` gains `prune: bool` (`FetchPrune::On`). Failures per remote are collected,
    not fatal: `FetchAllOutcome { failed: Vec<(String, TransferErrorKind)> }`.
  - Prune default reads `fetch.prune` / `remote.<name>.prune`.
- `branch.rs`: `BranchInfo` gains `ahead`/`behind` relative to its upstream (`graph_ahead_behind`,
  `None` without upstream).
- Not undoable (network operations), consistent with 3b.

### Frontend

- Tag badges on graph rows (distinct shape from branch badges); right-click a tag badge: push,
  delete, checkout (detached).
- Remote branch menu: "Delete <remote>/<branch>…" (confirm dialog, danger style).
- Local branch menu: "Push to…" (remote picker, remote branch name defaulting to the local
  name, "Set as upstream" checked when the branch has none). Pushing a branch with no upstream
  from the sync bar opens the same dialog instead of failing.
- Sync bar: Fetch becomes a split button: "Fetch", "Fetch all remotes", "Prune" checkbox
  (persisted).
- Branch tree rows show `↑n ↓m` for local branches with an upstream.
- Palette: "Fetch all remotes", "Delete remote branch…", "Push branch to…".

### Tests

Bare remote in a temp dir: delete remote branch removes both sides; push non-current branch
with and without upstream; fetch all with one unreachable remote reports partial failure; prune
removes a deleted remote branch's tracking ref; tags on graph rows (annotated peeled);
ahead/behind counts. E2E: publish a new branch, delete it remotely.

### Out of scope

Pushing to a different remote branch name than chosen in the dialog; tag badges for remote-only
tags; auto-fetch on a timer.

---

## 4c – Merge and stash options

### Backend

- `merge.rs`: `start_merge(repo, branch, opts: MergeOptions)` with
  `MergeOptions { fast_forward: FastForward /* Allow | Never | Only */, squash: bool }`.
  - `Never` → always a merge commit (skip the fast-forward branch of `merge_analysis`).
  - `Only` → `MergeError::NotFastForward` when a merge commit would be needed.
  - `squash` → merge into index and working tree, write `SQUASH_MSG` (log of squashed commits),
    `cleanup_state` so no `MERGE_HEAD` remains; the user commits normally. `HeadInfo.operation`
    unaffected.
  - Default from `merge.ff` config.
- `stash.rs`:
  - `save_stash(repo, opts: StashOptions)` with `message: Option<String>`,
    `keep_index: bool`, `include_untracked: bool` (default true, current behavior),
    `paths: Vec<String>` (non-empty → `stash_save_ext` with a pathspec).
  - `pop_stash(repo, index)`: apply with `StashApplyOptions::reinstantiate_index()`; drop only
    if the apply produced no conflicts.
  - `apply_stash` gains `restore_index: bool`.
  - `stash_diff(repo, index, opts: DiffOptions) -> StashDiff { tracked: Vec<FileDiff>,
    untracked: Vec<FileDiff> }` — stash commit vs its first parent, plus the untracked-files
    third parent.
  - Stash apply/pop conflicts: leave conflicts in the index and report
    `StashApplyOutcome::Conflicted { files }`; resolved through the existing conflict pane.
- Undo: squash merge and no-ff merge are recorded like merge; stash ops stay not undoable.

### Frontend

- "Merge into current branch" opens a small dialog: Fast-forward if possible / Always create
  a merge commit / Fast-forward only, and "Squash". Remembers the last choice per repo.
- Stash button becomes a split button: "Stash all", "Stash…" (dialog: message, keep staged,
  include untracked), and "Stash selected files" from a file's context menu (multi-select of
  changed files where supported).
- Stashes panel: click a stash to show its diff in the diff pane (read-only, with an
  Untracked group); Apply / Pop / Drop actions; "Restore staged changes" checkbox on apply.
- Palette entries for each.

### Tests

no-ff creates a two-parent commit where ff was possible; ff-only refuses; squash leaves staged
changes, `SQUASH_MSG`, clean state, HEAD unmoved; stash with message, keep-index (staged stays
staged and in stash), path-limited stash leaves other files; pop drops on success and keeps on
conflict; stash diff includes untracked files. E2E: stash one file with a message, view it,
pop it.

### Out of scope

Octopus merges, merge strategies (`-X ours`), stash branches (`git stash branch`), staged-only
stash.

---

## 4d – Compare

### Backend

- `diff.rs`: `range_diff_files(repo, from, to)` and `range_diff(repo, from, to, path, opts)`
  — tree-to-tree diff between any two commits (rename detection as in `commit_rename_diff`).
- `compare_working_tree(repo, commit, path, opts)` — working tree vs a commit.

### Plumbing

```ts
getRangeFiles(repoPath: string, fromId: string, toId: string): Promise<string[]>;
getRangeDiff(repoPath: string, fromId: string, toId: string, path: string, options: DiffOptions): Promise<FileDiff>;
```

### Frontend

- Selecting exactly two commits adds "Compare these commits" to the multi-select toolbar and
  menu; the diff pane shows `older..newer` with a header naming both and a swap button.
- Branch menu: "Compare with current branch" (merge-base..branch, labelled "changes on
  <branch> since it diverged") and "Diff against working tree" on a commit.
- Read-only; file history and blame available from the file headers.

### Tests

Range diff of non-adjacent commits; reversed order; merge-base compare; rename across the
range. Frontend: toolbar item only with exactly two selected; swap. E2E: compare two commits.

### Out of scope

Comparing across repositories; three-way compare views.

---

## 4e – Git hooks

### Backend (`git-core/hooks.rs`, new)

- `hooks_dir(repo)`: `core.hooksPath` (relative to the worktree root) or `<gitdir>/hooks`.
- `run_hook(repo, name, args, stdin: Option<&[u8]>) -> Result<HookOutcome, HookError>`:
  skips missing or non-executable files (Unix) / runs through `sh` found next to `git.exe` or on
  `PATH` (Windows; skipped with a logged notice when no `sh` exists). Working directory = worktree
  root, `GIT_DIR`/`GIT_INDEX_FILE` set like git does, combined stdout+stderr captured (capped at
  64 KiB), 10-minute timeout, killed on cancel. Same process-spawn pattern as `signing.rs`.
- Wired hooks:
  - `commit`/`amend_head`: `pre-commit` → `prepare-commit-msg` → `commit-msg` (message passed in
    a temp file, re-read after the hook so edits apply) → write commit → `post-commit` (failure
    ignored, output logged).
  - `push_refs`: `pre-push` with the remote name/URL as args and `<local ref> <local sha>
    <remote ref> <remote sha>` lines on stdin. It runs before connecting, so the remote SHA comes
    from the local remote-tracking ref (zero when absent) rather than the live advertisement —
    documented as a difference from git.
  - `switch_branch`/`checkout_commit`: `post-checkout`; merges: `post-merge`. Failures ignored.
- `CommitError::HookFailed { hook, output }` / `RemoteError::HookFailed { … }`.
- `no_verify: bool` parameter on commit, amend and push skips `pre-commit`, `commit-msg` and
  `pre-push`, matching `--no-verify`.
- Rebase and cherry-pick do not run commit hooks (matching git's defaults for `pick`).

### Frontend

- Hook failure shows a dialog with the hook name and its output (monospace, scrollable), plus
  "Commit without hooks" / "Push without hooks" retry buttons.
- CommitBox overflow: "Skip hooks" toggle for the next commit only. Palette entry.
- A spinner label "Running pre-commit hook…" while a hook runs (progress event from the worker).

### Tests

Real temp repos with shell-script hooks (Unix-only tests gated with `#[cfg(unix)]`): failing
pre-commit blocks the commit and returns output; commit-msg rewriting the message; `no_verify`
skips; `core.hooksPath` honored; pre-push blocks a push to a bare remote; post-commit failure
does not fail the commit. E2E: failing pre-commit shows output, "Commit without hooks" succeeds.

### Out of scope

Server-side hooks, `reference-transaction`, hook managers' config files, running hooks inside
rebase.

---

## 4f – External tools

### Backend (desktop: `tauri-app` commands; VSCode: extension host)

- `open_in_editor(repo, path, line?)`: `browsitory.editor` setting, else `core.editor` is **not**
  used (terminal editors), else the OS default app. `{file}`/`{line}` placeholders.
- `reveal_in_file_manager(repo, path)`: `tauri_plugin_opener::reveal_item_in_dir`.
- `open_terminal(repo)`: configurable command, sensible per-OS default (Terminal.app, Windows
  Terminal/`cmd`, `$TERMINAL`/`x-terminal-emulator`).
- `open_difftool(repo, path, staged | commit)` / `open_mergetool(repo, path)`: read
  `diff.tool`/`merge.tool` and `difftool.<tool>.cmd`/`mergetool.<tool>.cmd`; write
  `LOCAL`/`REMOTE`/`BASE` blobs to a temp dir, substitute `$LOCAL $REMOTE $BASE $MERGED`, run
  detached. After a merge tool exits 0 (or `trustExitCode=false` and the file changed), offer
  "Mark resolved" (stage the file).
- VSCode: editor → `vscode.open`; diff → `vscode.diff` with `git:` URIs; reveal →
  `revealFileInOS`; terminal → `window.createTerminal({ cwd })`; merge tool → VSCode's merge
  editor.

### Plumbing

```ts
openInEditor(repoPath: string, path: string, line: number | null): Promise<void>;
revealInFileManager(repoPath: string, path: string | null): Promise<void>;
openTerminal(repoPath: string): Promise<void>;
openDiffTool(repoPath: string, path: string, source: DiffSource): Promise<void>;
openMergeTool(repoPath: string, path: string): Promise<void>;
```

### Frontend

- File header and file context menu: "Open in editor", "Reveal in file manager", "Open in diff
  tool"; conflicted files: "Open in merge tool". Repo tab menu / palette: "Open terminal here",
  "Reveal repository".
- Settings: a small "External tools" form (editor command, terminal command) in a new
  Preferences dialog (palette "Preferences…"); diff/merge tool come from git config and are
  shown read-only with their source.

### Tests

`git-core`/`tauri-app`: tool command resolution and placeholder substitution (pure functions);
temp blob files written with correct contents. Process spawning itself is not unit tested.
Frontend: menu items, Preferences form persistence. No E2E (spawns external processes).

### Out of scope

Bundled diff/merge tools, per-file-type tool mapping.

---

## 4g – Conflicts, picks and polish

### Backend

- `merge.rs`: `resolve_whole_file(repo, path, side: Ours | Theirs)` checks out that stage into
  the working tree and stages it (works for content conflicts, not just add/delete).
  `mark_resolved(repo, path)` stages the working-tree file, refusing while conflict markers
  remain unless `force`.
- `pick.rs`:
  - `cherry_pick_many(repo, ids)` / `revert_many(repo, ids)` applied oldest-first (revert:
    newest-first) as a sequence, stopping at the first conflict with a persisted sequencer state
    (`<gitdir>/browsitory/sequence.json`, like rebase) and `continue_sequence` / `abort_sequence`.
  - `mainline: Option<u32>` for merge commits (`CherrypickOptions::mainline`,
    `RevertOptions::mainline`); `MergeCommitUnsupported` becomes `MainlineRequired`.
- `commit.rs`: `amend_head` gains `author: Option<SignatureInput>`; `commit` accepts trailers
  appended via `git interpret-trailers` semantics (blank line, `Key: value`).
- `diff.rs`: `blob_contents(repo, oid) -> Vec<u8>` (size-capped, 20 MB) for image preview.

### Frontend

- Conflict pane header: "Use ours (whole file)", "Use theirs (whole file)", "Mark resolved".
- Multi-select: "Cherry-pick N commits", "Revert N commits"; status strip shows
  "Cherry-picking 2/5" with Continue/Abort (reuses `OperationStatusStrip`).
- Merge commit cherry-pick/revert opens a parent picker.
- Image diff: PNG/JPEG/GIF/WebP/SVG (SVG rendered as an image, never inline) side-by-side with
  dimensions and size; swipe/onion-skin out of scope.
- CommitBox: summary character counter with a soft limit at 50 and a ruler at 72 for the body
  (configurable in Preferences); "Add co-author" picks from recent authors; amend shows an
  "Author" field (prefilled, editable).

### Tests

Whole-file ours/theirs on a content conflict; mark resolved refuses with markers; multi pick
sequence with a conflict in the middle, continue and abort restore correctly; mainline revert of
a merge; amend with author override keeps committer; trailer formatting; blob size cap.
Frontend: menu items and states, parent picker, image diff render, counter thresholds. E2E:
cherry-pick three commits with one conflict, resolve whole-file ours, continue.

### Out of scope

Image swipe/onion views, PDF preview, trailer config (`trailer.*`).

## Docs

Each batch updates `docs/USER_GUIDE.md` (new sections: "Searching commits", "Comparing
commits", "Hooks", "External tools", plus extensions to Branches/Stashes/Merge/Rewriting) and
`CHANGELOG.md` under Unreleased, and adds any new dependency to `docs/LICENSE_COMPLIANCE.md`.
