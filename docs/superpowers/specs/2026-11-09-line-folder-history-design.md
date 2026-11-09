# Line-range and folder history (#447)

## Goal

"Which commits changed these lines?" from a selection in a diff or blame view, plus a
`line:<path>:<from>-<to>` search key; "Folder history" for a file's folder.

## Backend

- `git_core::line_history::line_history(repo, revision, path, LineRange, limit)` walks
  `TOPOLOGICAL | TIME` from `revision`, carrying `(path, range)` per commit to its parents like
  `history::file_history` carries the path. libgit2 has no `log -L`, so the range is mapped
  through each parent's zero-context blob diff: unchanged lines map one to one, an edge inside a
  changed hunk maps to the hunk's old side (so the parent's range covers the old version of every
  change in it), lines all added by the commit end the walk on that parent.
- A commit is listed when it touched the range against every parent. A parent whose range is
  untouched (same blob, or a diff with no hunk in the range) explains the range: only that
  parent is followed (Git's default simplification for `log -L`).
- Renames via `history::rename_source`. Two children reaching one parent with the same path
  merge their ranges into the covering block.
- Cap: `MAX_LINE_HISTORY_COMMITS` (50,000) commits examined; the result says `truncated`.
- Errors: `HistoryError::{NotAFile, NotText, InvalidRange}` (plain `Other` error kind).
- Search: `SearchTerm::Lines`; `prepare` computes the line history's commit set once per page
  (from the `branch:` tip, else HEAD) and the term matches by membership, so it stays a
  per-commit predicate the boolean operators of #450 can combine.
- One read command `GetLineHistory` (policy `free`), wire DTOs `LineHistory`/`LineHistoryEntry`.

## Frontend

- `useFileHistory` holds a `HistoryTarget` (`file` or `lines`); `FileHistoryList` shows either.
- Diff and blame rows carry `data-line-number` (new side). `useSelectedLines` reads the
  browser's text selection inside a `ReadOnlyFileSection`; its header gets "History of lines
  N–M" (commit diffs, blame, commit-to-commit compares; not against the working tree, whose line
  numbers are no commit's).
- "Folder history" is a `path:<folder>/` search (`searchHistory`), in every file header.

## Limits (user guide, `file-history.md`)

One contiguous block (no split like `git log -L`), no copy detection, ambiguous diffs can shift
an edge by a line, the 50,000-commit cap.
