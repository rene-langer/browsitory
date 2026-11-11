### Drag a branch or commits onto a branch

- Added: drag a branch onto the current branch, or the current branch onto another branch, to
  merge or rebase. The drop opens a small menu with both choices, naming both branches, and they
  open the same merge dialog or rebase planner as the menu items. Drops that can't run (two
  branches that aren't current, a detached HEAD, a busy repository) are refused, and the label
  beside the pointer says why. Collapsed folders open while a branch hovers them (#744).
- Added: drag a commit from the graph onto the current branch in the sidebar to cherry-pick it.
  The drop opens a menu with "Cherry-pick ⟨commit⟩ onto ⟨branch⟩" and Cancel; a row of a
  multi-selection drags all selected commits (picked oldest first), and a merge commit opens the
  parent picker. Other branches, a detached HEAD, HEAD itself, a selection with a merge commit and
  a busy repository refuse the drop, and the label beside the pointer says why (#744).
