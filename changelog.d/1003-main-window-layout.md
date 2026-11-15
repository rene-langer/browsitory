### Main window layout

- Changed: an untracked file is marked **?**, as in `git status --short`, instead of **U**, which
  git uses for unmerged files. Above the diff, a file's state reads **Untracked**, **Modified, not
  staged** or **Added, staged** (#1027).
- Changed: a tab with uncommitted changes shows **±** and the number of changed files instead of a
  dot that looked like an unsaved-file marker; the tooltip and the tab's accessible name say it in
  words (#1028).
- Changed: the start page puts the keyboard focus in the filter (or on **Open repository…**), and
  a dialog starts on its first control, a radio group on its chosen option, never on its frame.
  The focus ring is blue with a small gap, so a focused button no longer looks like a pressed
  toggle (#1029).
- Changed: the commit box is one line with **Commit** beside it until you click into the message,
  so the diff above gets the room; it stays open while it has the focus or holds anything, and it
  never takes more than half the column. **Add co-author…** and **Amend last commit** moved into
  the menu of the arrow next to **Commit**, which now opens without a message; **Cancel amend**
  ends amend mode (#1003).
- Changed: a clean working tree's commit box no longer says "Stage changes to commit" in amber: it
  says "Nothing to commit.", and "Stage changes to commit." only when there are changes to
  stage, both in muted text (#1005).
- Changed: the commit graph starts at about 40% of the window, so a wide window shows longer
  subjects. Its date column is compact ("3h", "2d", "4mo"; the date in the tooltip), and a row
  with several branches and tags shows the first and "+N" for the rest (#1004).
