### Stashes in sparse checkouts

- Fixed: in a sparse checkout, Stash recorded the files outside the chosen folders as deleted,
  wrote them to disk and dropped their mark; Pop then deleted them again and the change list
  showed them as deleted, so Stage all could commit their deletion. Stash now leaves them alone,
  as `git stash` does, and so it does with files marked with `git update-index --skip-worktree`
  (#1184).
- Fixed: applying or popping a stash that changes a file outside the sparse checkout (made
  before it was narrowed, say) left that change out without a word, and Pop then dropped the
  stash. The file is now written with the stashed change, as with `git stash apply`. A file
  whose staged change Apply or Pop brings back outside the chosen folders stays outside, as
  with git (#1184).
