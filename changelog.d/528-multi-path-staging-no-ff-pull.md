### Staging several files, no-ff pulls

- Changed: staging, unstaging or ignoring several files at once (the selection bar, Stage all,
  Unstage all) is one Undo step ("Stage 3 files") instead of one per file, and all or nothing:
  when one file can't be staged, none is. The notification after ignoring several files now
  offers Undo (#528).
- Fixed: Pull follows `pull.ff=false`, `--no-ff` in `branch.<name>.mergeoptions` and
  `merge.ff=false` like `git pull`: a branch that could fast-forward gets a merge commit instead,
  ready to commit in the commit box. A configured rebase still fast-forwards (#740).
