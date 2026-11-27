### Sparse checkouts in merges and switches, and Pull with local changes

- Fixed: in a sparse checkout, merging, cherry-picking, reverting or rebasing a commit that
  changes a file outside the chosen folders was refused ("uncommitted change would be
  overwritten"), and a file it added there was written to disk. The changed file now keeps its
  new version in the index without being written, a new one isn't written either, and a conflict
  in such a file is written with its markers so it can be resolved, as with `git merge` (#1185).
- Fixed: in a sparse checkout, switching to a branch (or checking out a commit) that changes a
  file outside the chosen folders was refused as "local changes", and one that added a file there
  wrote it to disk; both now work as `git switch` does. A refused switch no longer names the
  missing files outside the folders (#1191).
- Fixed: Pull refused whenever any file was changed or untracked, even when the pull wouldn't
  touch it, and always in a sparse checkout. As with `git pull`, it now refuses only when a file
  it changes has local changes or an untracked file is in its way, and names those files; Undo of
  the pull keeps your local changes (#1152).
