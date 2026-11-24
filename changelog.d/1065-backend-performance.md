### Big files and many refs

- Changed: Switch, Pull, Merge, Rebase, Reset (soft or mixed), Checkout, Cherry-pick, Revert
  and Ignore no longer save a copy of a changed or untracked file over 50 MB for Undo; they
  note its size and time instead, so a big dump or dataset next to your code no longer slows
  every click. Undo still works and leaves such a file as it is; if it would have to bring back
  the old contents of one that changed since, it says so and changes nothing. Discards, Reset
  hard, restoring a file and applying a patch or a commit's change still save every file they
  replace, however big. Files Undo saves are no longer held in memory whole (#1065).
- Changed: a file over 20 MB on either side isn't diffed until you ask: the diff says how large
  it is and offers **Load diff anyway**, so opening one no longer holds up the app (#1066).
- Fixed: Delete branch took seconds in a repository with thousands of tags or branches while it
  counted the commits only that branch has, and a second click started a second count and a
  delete that failed. The count is now quick, and Delete is unavailable while it runs (#1067).
- Fixed: every refresh walked each stash's whole untracked folder again; a stash's file count is
  now worked out once (#1070).
