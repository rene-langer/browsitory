# Blame Viewer Design

Status: Approved

## Context

Branch management and stash (see `docs/superpowers/specs/2026-08-12-branch-management-design.md`
and `docs/superpowers/specs/2026-08-12-stash-design.md`) shipped the first two of Phase 2's six
subsystems. This spec covers the third: a per-line blame viewer. Phase 2's remaining three
(merge, rebase, commit graph) are out of scope here, each its own future spec.

**Goals:**
- View per-line author/commit/date blame for any tracked file, reachable from a "Blame" button
  next to each file in `DiffPane`'s existing file lists (both the uncommitted-changes pane and a
  commit's file list).
- Blame targets HEAD when opened from the uncommitted pane, or the selected commit when opened
  from a commit's file list.
- Clicking a blame line selects that line's commit in `HistoryList`, showing its diff in
  `DiffPane` (blame view stays open for the file until the user switches back to diff view or
  picks a different file).
- Blame replaces the diff view in place — a toggle within the same per-file display area, not a
  new panel or route.

**Non-goals (explicitly deferred):**
- Re-blame-before-this-commit drill-back (GitHub's "view blame prior to this change") — a fixed
  blame at one revision only, this pass.
- Blaming an uncommitted edit's actual working-tree content — blame is inherently a
  committed-history view; the uncommitted pane's "Blame" button always blames the file's HEAD
  version, not the dirty edit, regardless of whether the file is staged/unstaged/modified.
- A file-tree browser — blame is reachable only from files already listed in the current
  uncommitted-changes or commit file list, not for arbitrary tracked files.

## Architecture

### `git-core` addition: `blame.rs`

Same shape as other modules: a `thiserror` `BlameError` enum, tested against real temp-dir repos
(`crates/git-core/tests/blame.rs`).

```rust
pub struct BlameLine {
    pub line_number: usize,   // 1-indexed, matching git2's own blame line numbering
    pub content: String,
    pub commit_id: String,
    pub short_id: String,
    pub author_name: String,
    pub timestamp: i64,       // Unix seconds, UTC — matches CommitInfo's existing convention
}

pub fn blame_file(repo: &Repository, commit_id: &str, path: &str) -> Result<Vec<BlameLine>, BlameError>;
```

- `commit_id` resolves the same way `branch::resolve_start_point` already does: `"HEAD"` via
  `repo.head()`, otherwise `repo.revparse_single(commit_id)` — both peeled to a `Commit` whose
  `Oid` becomes `BlameOptions::new().newest_commit(oid)`. Scoping blame to `newest_commit` is what
  makes "blame at this historic commit" show the file as it existed there, not as it exists at
  HEAD.
- File content at that commit comes from `commit.tree()?.get_path(Path::new(path))?
  .to_object(repo)?.peel_to_blob()?.content()`, decoded via `String::from_utf8_lossy` and split on
  `\n` — the existing `diff.rs`/`status.rs` modules only ever produce diff hunks, never raw file
  content, so this is new machinery, but it's a direct, well-trodden git2 pattern.
- `repo.blame_file(path, Some(&mut opts))` gives a `Blame` handle; for each content line (1-based
  line number), `Blame::get_line(line_number)` returns the owning `BlameHunk`, from which
  `final_commit_id()`, `final_signature()` (author name via the same
  `.ok().flatten().unwrap_or_default()` defensive pattern `log.rs` already uses for
  `Signature::name()`/`email()` — see `CLAUDE.md`'s git2 gotchas), and the timestamp (via
  `final_signature()`'s own `.when().seconds()` — a `Time`/`i64` already attached to the
  signature, no separate `find_commit` lookup needed) are read.
- `blame_file` takes `&Repository`, not `&mut Repository` — no threading wrinkle like stash's.

### `tauri-app`: `Worker`/`Command` and Tauri command

One new operation, following the exact existing pattern: `Command::GetBlame { commit_id: String,
path: String, reply: Sender<Result<Vec<BlameLine>, String>> }` → a `WorkerHandle::get_blame`
method → a `#[tauri::command] get_blame` pass-through in `commands.rs` (with a `BlameLineDto`,
camelCase-serialized) → registered in `main.rs`.

### `RepoClient` / frontend IPC

```ts
export interface BlameLine {
  lineNumber: number;
  content: string;
  commitId: string;
  shortId: string;
  authorName: string;
  timestamp: number;
}

getBlame(commitId: string, path: string): Promise<BlameLine[]>;
```

### Frontend state and components

`DiffPane.tsx` gains a required `onSelectRow: (row: SelectedRow) => void` prop (not currently on
`DiffPane` — needed so a clicked blame line can move `HistoryList`'s selection).

Both `UncommittedDiffPane` and `CommitDiffPane` gain a `viewMode: "diff" | "blame"` local state,
alongside their existing `selected`/`selectedPath` state. Clicking a file's own name (existing
behavior) selects the file and sets `viewMode: "diff"`. A new "Blame" button next to each file row
selects the file and sets `viewMode: "blame"`, triggering a `client.getBlame(...)` fetch
(`"HEAD"` in `UncommittedDiffPane`'s case, `commitId` in `CommitDiffPane`'s case) — same
`useEffect` + `ignore`-guard pattern the existing diff fetches already use. When `viewMode ===
"blame"`, render the new `BlameView` component instead of `DiffView`; a small toggle (e.g. a
"Diff"/"Blame" pair of buttons, or a single "Back to diff" button while in blame mode — left to
implementation, not a design-level decision) switches back.

**`BlameView.tsx`** (new component): renders one row per `BlameLine` — line number, `shortId`,
`authorName`, `content` — each row's click calls `onSelectRow({ commitId: line.commitId })`.

### Error handling

No new plumbing: blame errors flow through the same `Result<T, String>` → rejected promise →
`state.error` → inline banner path every other feature already uses.

### Testing

- `crates/git-core/tests/blame.rs`: a single-commit file (every line attributed to that one
  commit), a multi-commit file where a later edit only reattributes the changed lines (earlier,
  untouched lines keep their original commit's id), blame scoped to a specific historic commit via
  `commit_id` (lines added by a later commit aren't present, since the file didn't have them at
  that revision), and a file-not-found error case.
- `crates/tauri-app/src/worker.rs`: one thin wiring round-trip test, matching the economical scope
  every other feature's worker tests already use.
- `frontend/src/components/BlameView.test.tsx` (new): renders rows with line/author/content;
  clicking a row calls `onSelectRow` with the right `commitId`.
- `frontend/src/components/DiffPane.test.tsx` additions: clicking "Blame" fetches and renders
  `BlameView` instead of `DiffView`; switching back to diff view restores `DiffView`.
- One new E2E flow (`e2e/specs/`): commit a file, edit and commit it again, open blame on it, see
  two distinct commit ids attributed to different lines, click a line, see `HistoryList`'s
  selection jump to that line's commit.
