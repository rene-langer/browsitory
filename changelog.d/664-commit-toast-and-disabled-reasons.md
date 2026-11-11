### Commit notification, disabled reasons and the working tree

- Changed: after a commit the notification offers **Push** and **Undo** together, not one or the
  other (#733).
- Fixed: the commit notification's **Push** pushed whichever branch was current when it was
  clicked; it now pushes only the branch it names and otherwise says "Not pushed: main is no
  longer the current branch." (#735).
- Fixed: in the working tree, the Changes heading broke its labels over two lines and the diff
  options toolbar was cut in half when the file list grew (#752).
- Fixed: "Clone repository" opened as a dialog inside another dialog, announced twice by screen
  readers (#761).
- Changed: the buttons of the sidebar panels (worktrees, tags, pull requests, branches, remotes),
  the operation status strip, the rebase planner, the files-at-a-commit view and the remaining
  dialogs and confirmations say why they can't run in visible text tied to the button, not only
  in a tooltip (#664, #748).
