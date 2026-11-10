### Notifications and Undo

- Changed: notifications have a tone (done, information, warning, error), each with its own
  colour and icon; warnings and errors also say so to screen readers. They stay 5 to 15 seconds
  depending on their length, at least 10 seconds with a button, and errors stay until closed.
  None close while the pointer is over one or focus is on one of its buttons (#661).
- Changed: notifications name what changed: "Cherry-picked 1234567", "Reset main to 1234567",
  "Checked out 1234567", "Switched to topic", "Created tag v1", "Pulled origin/main" (#661).
- Added: the notification of an operation Undo covers (commit, amend, discards, staging lines,
  reset, checkout, branch switch and delete, tag creation, fast-forward, squash merge,
  cherry-pick, revert, history edits, restoring or applying a file) has an Undo button. It
  undoes only that operation, and says so instead when something newer was recorded (#662).
- Fixed: the discard, Delete untracked, Reset hard and force-delete branch confirmations said
  the action can't be undone; Undo restores all of them, and they now say so (#662).
- Changed: Discard hunk and Discard lines (and `d` in a diff) no longer ask twice: they discard
  at once, and the notification offers Undo (#662).
