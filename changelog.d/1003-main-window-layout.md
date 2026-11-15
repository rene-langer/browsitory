### Main window layout

- Changed: an untracked file is marked **?**, as in `git status --short`, instead of **U**, which
  git uses for unmerged files. Above the diff, a file's state reads **Untracked**, **Modified, not
  staged** or **Added, staged** (#1027).
- Changed: a tab with uncommitted changes shows **±** and the number of changed files instead of a
  dot that looked like an unsaved-file marker; the tooltip and the tab's accessible name say it in
  words (#1028).
