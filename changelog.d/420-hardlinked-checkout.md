### Hard-linked files

- Fixed: a tracked file that is a hard link to a file elsewhere (another folder, or outside the
  repository) changed under its other name too when Browsitory wrote it through libgit2:
  Discard (file, hunk or lines), branch switches, resets, merges, cherry-picks, reverts, rebase
  steps, Stash and stash apply or pop, aborts, Undo/Redo of a staged path and submodule
  updates. As with git, the file is now replaced: the work-tree name gets the new content and
  the other name keeps its old content, on Linux, macOS and Windows (#420).
