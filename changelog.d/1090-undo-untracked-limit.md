### Undo next to many untracked files

- Changed: with more than 10,000 untracked files (a build folder or dataset that isn't ignored),
  Switch, Pull, Merge, Rebase, Reset (soft or mixed), Checkout, Cherry-pick, Revert and Ignore no
  longer save a copy of each untracked file for Undo; they note each file's size and time
  instead, so those commands no longer read the whole folder twice. Undo still works and leaves
  those files as they are; if it would have to bring back the old contents of one that changed
  since, it says so and changes nothing. Discards, Reset hard, restoring a file and applying a
  patch or a commit's change still save every file, however many (#1090).
- Changed: the undo history file has a new format version. A Browsitory version from before
  this change starts with an empty undo history in a repository a newer version used, instead of
  misreading it, and leaves the newer history untouched (#1093).
