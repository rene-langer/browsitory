### File/folder conflicts libgit2 misses

- Changed: a merge, squash merge, cherry-pick or revert where a file and a folder of the same name
  meet and libgit2 sees no conflict (a name sorts between them, as `c`, `c.txt`, `c/g`) no longer
  refuses. Like git, it merges the folder's files and keeps your file as the conflict `c~HEAD`
  (`c~<branch>` when the file comes from the other side), which you resolve or abort (#416, #410).
  A rebase step and a stash apply still refuse and point to git.
