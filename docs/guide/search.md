# Searching commits

The search field sits above the commit graph. Click it, press **Ctrl+F** (**Cmd+F** on macOS)
while the commit list has focus, or run **Search commits** from the command palette. Once you
stop typing, the graph is replaced by the matching commits and the first match is selected.
Press ↓ in the field to move into the results.

| Syntax | Meaning |
|---|---|
| `word` | Message contains it, or SHA starts with it |
| `"two words"` | Exact phrase |
| `author:NAME` | Author name or email |
| `committer:NAME` | Committer name or email |
| `message:TEXT` | Anywhere in the full message |
| `commit:SHA` | SHA starts with it |
| `path:PATTERN` | Changes a matching path (`src/`, `*.md`) |
| `contents:TEXT` | An added or removed line contains it |
| `line:PATH:FROM-TO` | Changed those lines of the file at HEAD, followed back |
| `after:YYYY-MM-DD` | Committed on or after that day |
| `before:YYYY-MM-DD` | Committed on or before that day |
| `branch:NAME` | Only commits reachable from NAME |

- Terms combine with AND. Case is ignored except in `path:`.
- Quote text with spaces or a colon (`"fix: crash"`). An unquoted `word:` that is not one of the
  keys above is an error, so a conventional-commit prefix like `fix:` needs quotes.
- Dates are whole days in the commit's own timezone, inclusive.
- `path:` and `contents:` look at what a commit changed compared with its first parent, so a
  merge counts as the changes it brought in.
- Without `branch:`, the search covers the commits the graph shows (see
  [Which branches the graph shows](history.md#which-branches-the-graph-shows)), remote branches
  you added to the graph included; with a single branch in the graph, only that branch's own
  line, as in the graph. `branch:NAME` always
  searches every commit reachable from NAME, merged branches included.
- `line:src/main.rs:10-20` (or `line:src/main.rs:12` for one line) lists the commits that
  changed those lines, following them back through earlier edits, the same as
  [line history](file-history.md#history-of-lines) does. The line numbers are the file's at HEAD,
  or at the tip of the `branch:` term's branch. Put a path with spaces in quotes
  (`line:"my file.txt:3-4"`). Lines outside the file are an error.
- Results load 100 at a time, and "Scanned N commits…" shows progress on large histories.
- Selecting a result shows its diff as usual.
- Esc or × goes back to the graph and the previous selection.
- File history opened from a result returns to the results when closed.
