### File/folder conflicts libgit2 misses

- Changed: a merge, squash merge, cherry-pick, revert, rebase step or stash apply (or pop) where a
  file and a folder of the same name meet and libgit2 sees no conflict (a name sorts between
  them, as `c`, `c.txt`, `c/g`) no longer refuses. Like git, it merges the folder's files and
  keeps your file as the conflict `c~HEAD` (`c~Updated upstream` for a stash), which you resolve
  or abort; a file coming in is moved to git's name for the other side (`c~<branch>`, with a `/`
  as `_`; `c~<commit> (<subject>)`; `c~Stashed changes`). A stash apply restoring the staged
  changes refuses, as `git stash apply --index` does (#416, #410).
