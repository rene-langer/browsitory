### Drag a branch onto a branch

- Added: drag a branch onto the current branch, or the current branch onto another branch, to
  merge or rebase. The drop opens a small menu with both choices, naming both branches, and they
  open the same merge dialog or rebase planner as the menu items. Drops that can't run (two
  branches that aren't current, a detached HEAD, a busy repository) are refused, and the label
  beside the pointer says why. Collapsed folders open while a branch hovers them (#744).
