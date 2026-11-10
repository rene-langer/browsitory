### Fetch, pull and push without a blocking dialog

- Changed: a running fetch, pull or push no longer covers the window with a dialog. The
  repository toolbar names it ("Pushing main to origin/main") with a progress bar, the objects
  and bytes so far, and Cancel; the history, the sidebar and what is already shown stay usable.
  Actions that change the repository wait until it ends, and their buttons say what for (#659).
- Fixed: Escape closed the transfer dialog, and its progress and Cancel were gone for the rest
  of the transfer (#659).
- Added: an action that takes more than a moment shows "Working…" with a progress bar in the
  toolbar, and the graph and the Branches section show placeholder rows while a repository
  opens, instead of looking empty (#660).
- Changed: fetch, push and pull notifications say what happened: "Pushed 2 commits to
  origin/main", "Nothing to push: origin/main already has main", "Fetched from origin. main is
  3 commits behind origin/main." with a Pull button, "Fast-forwarded main to origin/main",
  "Deleted origin/topic". The toolbar's Fetch and Push and the palette's now notify too (#661).
