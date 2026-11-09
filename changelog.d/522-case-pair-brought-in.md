### Files that differ only in case

- Fixed: with `core.ignorecase` on (Linux), a rebase step, cherry-pick, revert, merge, pull or
  stash apply that added a file whose name differs only in case from an existing one (`a` next
  to `A`) silently kept only one of the two in the commit and the index. It is now refused
  before anything is written and names both files; a rebase or a cherry-pick of several
  commits stops at that step and can be aborted. Aborting a cherry-pick of several commits after
  such a step had landed outside Browsitory now goes back to where it started (#522).
