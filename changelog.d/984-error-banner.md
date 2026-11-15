### Errors

- Changed: the error banner is one row: the message, its next steps, **Details** and ✕. A
  rejected push offers **Pull** first and **Force push…** second; **Details** opens the operation
  log on the failed operation, where its full text and **Copy details** are. An error with no
  operation behind it (reading the repository failed) offers **Copy details** in the banner
  instead, with the full text (#984).
- Fixed: the banner's filled first step (**Pull**, **Remove**) and its ✕ no longer lose their
  colours on hover, which hid their labels (#984).
- Fixed: no error banner while a dialog handles the same error. A missing git identity showed two
  near-identical lines beside the identity dialog and stayed after Cancel; a failed hook repeated
  the dialog's first line. Rename branch, Connect to GitHub and New pull request show their
  failure in the dialog only, also after Cancel (#996).
- Changed: Delete branch counts the commits only that branch has before it deletes. With none, it
  deletes at once; otherwise one confirm says "feature has 1 commit on no other branch. Delete it
  anyway?", instead of a failed plain delete's error banner next to a Force delete confirm (#996).
- Changed: errors name the operation, what it worked on and the next step: "Couldn't fetch from
  origin. Check the remote's URL and your connection, then try again." instead of "Fetch failed",
  and "Cherry-pick of 4c5beaf didn't start: …". libgit2's `class=…; code=…` is left out of the
  banner; the operation log and **Copy details** keep it (#997).
- Changed: the remaining error banners name the operation and what it worked on, then Git's
  reason, then the next step: "Couldn't switch to feature/x: your local changes to src/map.js would
  be overwritten" with "Commit or stash the changes to those files first, or use Stash and switch."
  This covers switching branches and commits, creating, renaming and deleting branches and tags,
  stash save, apply, pop and drop, reset, commit and amend (also when a hook fails), merge, rebase,
  history edits, cherry-picks and reverts of several commits, worktrees, submodules, patches and
  restoring a reflog entry. The create forms show the same text. The operation log keeps Git's
  full message (#1032).
