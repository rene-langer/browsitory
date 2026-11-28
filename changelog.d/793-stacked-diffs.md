### Working tree: all diffs at once

- Added: **Stacked** next to **Flat** and **Tree** shows every listed file's diff one below the
  other, with the file list above as an index: picking a file scrolls to its diff, **Enter**
  moves into it, and `j`/`k` go between diffs. Every diff has hunk and line staging, each loads
  when it scrolls near the view, and the stack adds 100 files at a time. **List** goes back to
  one viewer; the choice is kept on this computer, also in **Preferences → Diff** (#793).
- Fixed: a failed update check or download (desktop app) now writes its reason to
  `browsitory.log`; the automatic checks still show nothing (#1084).
