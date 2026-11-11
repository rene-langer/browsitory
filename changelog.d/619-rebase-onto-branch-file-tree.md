### Rebase onto a branch, file list tree and filter

- Added: **Rebase current branch onto ⟨branch⟩…** in the menus of local and remote branches and
  in the command palette opens the rebase planner on that branch, named in the planner's title.
  It is disabled with the reason during a merge, rebase or cherry-pick and not offered on a
  detached HEAD (#619).
- Changed: **Undo** names a rebase onto a branch by the branch ("Rebase onto origin/main"), also
  for **Rebase** after a pull has diverged, instead of the full ref name (#619).
- Added: **Filter files** above the working tree's file list narrows both groups by path. While it
  hides files, the group buttons act on the shown files only and say so (**Stage 3 shown**,
  **Discard 3 shown…**, whose confirmation lists those files), and Ctrl+A selects only the shown
  files (#729).
- Added: **Flat** and **Tree** switch the file list to a tree of folders, with collapsible folders
  and a stage or unstage button per folder. The choice is kept on this computer and is also in
  **Preferences → Diff** (#729).
