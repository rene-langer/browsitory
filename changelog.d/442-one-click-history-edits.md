### One-click history edits

- Added: the commit menu edits a commit of the current branch in one step: **Reword…**,
  **Squash with parent**, **Fixup into parent**, **Move up**, **Move down** and **Drop**. Each
  runs as a one-step interactive rebase, so a conflict pauses it like any rebase and a finished
  edit is one Undo step. An edit that can't run is disabled with the reason (another branch, a
  merge commit, the first commit, uncommitted changes, an operation in progress). The palette
  has them for the selected commit too (#442).
- Changed: **Edit commit message…** on the latest commit is now **Reword…**; it still opens the
  amend box (#442).
