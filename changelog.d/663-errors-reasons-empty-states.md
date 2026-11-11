### Errors, disabled reasons and empty states

- Changed: error banners sit between the toolbar and the columns instead of covering the toolbar
  or the top of the graph (#663).
- Added: error banners have **Copy details**, and an error you didn't dismiss is listed under the
  next one as an earlier error instead of being replaced (#663).
- Added: hints with the next step for every error kind that has one, such as "Commit or stash
  your changes, then try again." (#663).
- Changed: a toolbar button that can't run (Fetch, Pull, Push, Stash, Undo, Redo) stays
  focusable and shows why under itself on hover and focus; a disabled menu item shows its reason
  under its label; dialogs' buttons are described by the reason they show (#664).
- Added: a search in the command palette also lists the matching commands that can't run now,
  greyed out under **Unavailable now** with the reason (#644).
- Added: **Working tree clean** and, in a repository without commits, **No commits yet** with
  how to make the first commit, instead of an empty pane (#665).
- Added: conflicted files head **Changes** in a **Conflicts** group (#665).
- Added: the status strip says what is merged into which branch ("Merging feature/x into main",
  "Cherry-picking onto main") and has **Go to conflict**; during a rebase it also has **Continue**
  (#665).
