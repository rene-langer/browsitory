### Errors

- Changed: the error banner is one row: the message, its next steps, **Details** and ✕. A
  rejected push offers **Pull** first and **Force push…** second; **Details** opens the operation
  log on the failed operation and replaces both "Show details" and "Copy details", which moved into
  the log (#984).
- Fixed: no error banner while a dialog handles the same error. A missing git identity showed two
  near-identical lines beside the identity dialog and stayed after Cancel; a failed hook repeated
  the dialog's first line (#996).
- Changed: Delete branch counts the commits only that branch has before it deletes. With none, it
  deletes at once; otherwise one confirm says "feature has 1 commit on no other branch. Delete it
  anyway?", instead of a failed plain delete's error banner next to a Force delete confirm (#996).
- Changed: errors name the operation, what it worked on and the next step: "Couldn't fetch from
  origin. Check the remote's URL and your connection, then try again." instead of "Fetch failed",
  and "Cherry-pick of 4c5beaf didn't start: …". libgit2's `class=…; code=…` is left out of the
  banner; Copy details and the operation log keep it (#997).
