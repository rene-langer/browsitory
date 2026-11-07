### File/folder conflicts libgit2 misses

- Fixed: a merge, squash merge, pull with merge, cherry-pick, revert, rebase step or stash apply
  where one side has a file and the other a folder of the same name, with another name sorting
  between them (`c`, `c.txt`, `c/g`), no longer loses a file. libgit2 merged the two without a
  conflict, so a cherry-pick, revert or rebase step committed without the file, and a merge or
  stash apply stopped half-way with "directory exists". Browsitory now refuses such an operation
  before changing anything and says to run it with git, which reports the conflict (#410).
