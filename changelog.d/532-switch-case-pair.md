### Files that differ only in case

- Fixed: with `core.ignorecase` on (Linux), switching to a branch, creating a branch from one,
  checking out a commit, tag or remote branch, or adding a worktree, whose files include two
  names that differ only in case (`A` and `a`) that the current commit doesn't, kept only one
  of the two in the index and didn't write the other's file, so the next commit dropped it. It
  is now refused before anything is written and names both files (#532).
