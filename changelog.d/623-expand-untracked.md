### Expand untracked files preference

- Added: **Preferences → Diff → Expand untracked files** (on by default). Off, untracked files
  start collapsed in the uncommitted changes and load nothing until expanded; changes to tracked
  files still start expanded. A file you expand or collapse keeps that state across refreshes
  for as long as the pane is open. Stored as `[diff] expand_untracked` (#623).
