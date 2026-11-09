### Aborting a cherry-pick or revert sequence

- Fixed: aborting a sequence whose steps left a pair of file names that differ only in case
  (with `core.ignorecase` on) unstaged changes staged on other files the rewind doesn't
  change. They now stay staged, as they do when no such pair is involved (#553).
