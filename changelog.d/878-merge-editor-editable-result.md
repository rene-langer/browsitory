### Merge editor

- Added: in the three-pane merge editor the whole result is editable, unchanged lines included.
  A block without a choice shows git's conflict markers there; each result line is marked with
  where it came from (ours, theirs, the base, typed, or still in conflict), and typing into a
  resolved block marks it Edited. Save resolution stays disabled while a `<<<<<<<` or `>>>>>>>`
  line is left (#878).
- Added: Undo and Redo in the merge editor (also Ctrl+Z, Ctrl+Shift+Z and Ctrl+Y while the focus
  is in it) step through typing, take-side arrows, choices and Reset; keystrokes in one place
  are one step (#878).
