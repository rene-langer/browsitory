### Sparse checkouts

- Fixed: in a sparse checkout, Reset to a commit that adds files outside the chosen folders no
  longer lists them as deleted (mixed; Stage all could then commit their deletion) or writes
  them to disk (hard): they are marked as outside the sparse checkout, as with `git reset`. The
  same for the files Abort merge, cherry-pick or revert and Undo put back. Abort rebase no
  longer writes every file outside the sparse checkout and drops their mark. With non-cone
  patterns (`git sparse-checkout set --no-cone`), such a file is treated as outside whenever it
  isn't on disk (#1172).
