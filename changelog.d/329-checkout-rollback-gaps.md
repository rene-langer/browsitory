### Stopped checkouts

- Fixed: a cherry-pick, revert, rebase, pull or stash apply whose checkout stopped while writing a
  file (a full disk, an I/O error) left that file cut short, so trying again was refused; it is
  now put back with the other files (#329).
- Fixed: a rebase step after the first whose checkout stopped part-way (a file open in another
  program, a read-only file) left the files it had written and showed git's raw error; the step
  is now rolled back, a file open in another program is named, and Continue applies the step
  again (#329).
- Fixed: rolling back a stopped checkout no longer removes an empty folder you already had, and
  removes the conflict files git had written under other names (`f.txt~HEAD`) and its leftover
  `.lock` file (#329).
