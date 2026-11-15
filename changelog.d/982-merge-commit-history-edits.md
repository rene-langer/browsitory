### History edits and merge commits

- Fixed: a commit below a merge commit said to use "Rebase onto here…", which can't edit it. Its
  edits now say "Can't edit: a merge commit comes after this one." (#982).
- Fixed: the rebase planner marks a merge commit's row ("Merge commit: can't be replayed"),
  offers it no action, and Start rebase is unavailable with the reason, instead of failing once
  started (#982).
