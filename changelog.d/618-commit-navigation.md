### Commit navigation

- Added: Back and Forward through the commits a tab selected: the ← and → buttons above the
  history, Alt+Left/Right (Cmd+[ / Cmd+] on macOS) where the host doesn't use them, the mouse's
  back and forward buttons, and "Back to previous commit" / "Forward to next commit" in the
  command palette. Moving through a list with the arrow keys counts as one step; a commit no
  longer shown is skipped (#618).
- Added: `[` and `]` in the commit list select the commit's first parent and its child (of
  several, the one on the same lane, else a list to choose from); Shift+`[` lists a merge
  commit's parents. A parent older than the loaded history is found as Go to does (#618).
- Added: the commit details list the commit's loaded children as links next to its parents
  (#618).
