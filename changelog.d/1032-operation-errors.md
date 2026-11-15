### Errors

- Changed: the remaining error banners name the operation and what it worked on, then Git's
  reason, then the next step: "Couldn't switch to feature/x: your local changes to src/map.js would
  be overwritten" with "Commit or stash the changes to those files first, or use Stash and switch."
  This covers switching branches and commits, creating, renaming and deleting branches and tags,
  stash save, apply, pop and drop, reset, commit and amend (also when a hook fails), merge, rebase,
  history edits, cherry-picks and reverts of several commits, worktrees, submodules, patches and
  restoring a reflog entry. The create forms show the same text. The operation log keeps Git's
  full message (#1032).
