### Paused operations

- Changed: the status strip is the only place for a paused merge's, cherry-pick's, revert's or
  rebase's status, Continue and Abort. The commit area no longer repeats them; at an edit stop
  it keeps the commit message (#995).
- Changed: the rebase strip names the branch and what it is rebased onto ("Rebasing main onto
  feature/x, step 1 of 2"), and the toolbar says "Rebasing main" instead of a detached HEAD
  (#995).
- Changed: Edit commit… and Split commit… name their controls and toasts after the edit:
  "Finish editing" / "Cancel edit", "Finish split" / "Cancel split", "Edited 1a2b3c4". "Step 1
  of 1" is gone, and Split says what to do with the parts (#994).
