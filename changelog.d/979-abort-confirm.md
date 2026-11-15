### Aborting a paused operation

- Changed: Abort merge, cherry-pick, revert or rebase asks first when it would throw away
  conflict resolutions, unsaved edits in the merge editor, files changed at an edit stop, a new
  commit message or a split's committed parts, which Undo can't bring back. When nothing was
  touched it still aborts at once (#979).
- Added: a toast says "Aborted merge" (or cherry-pick, revert) once the abort worked (#979).
