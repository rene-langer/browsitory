### Index integrity

- Fixed: Reset (mixed and hard) and Abort merge, cherry-pick or revert no longer drop the mark of
  files outside a sparse checkout or marked with `git update-index --skip-worktree`, which then
  showed up as modified or deleted. Reset hard leaves such a file alone when the commit has the
  same version of it, doesn't bring back the files outside a sparse checkout, and refuses
  (changing nothing) when it would overwrite a marked file's local edits, as `git reset --hard`
  does; before, it overwrote them and Undo couldn't bring them back. Reset hard also keeps a
  file added with `git add -N` instead of deleting it (#1145).
- Fixed: in a sparse checkout the files outside the chosen folders are no longer listed as
  deleted, and Stage all no longer stages their deletion; such a file can't be staged, as with
  `git add` (#1146).
- Fixed: a file added with `git add -N` is listed as an untracked file instead of a staged one,
  and a commit, amend or the other commands that commit leave it out instead of committing it
  as an empty file (#1164).
- Fixed: a stage that failed because another program held the index lock no longer takes effect
  later with the next command that writes the index (#1166).
- Fixed: staging, unstaging or resolving in the app while a merge, cherry-pick, revert or rebase
  is paused no longer undoes what was staged in a terminal or another tool since the app last
  read the status (#1167).
