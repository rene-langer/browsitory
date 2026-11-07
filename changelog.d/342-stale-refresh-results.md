### Refresh

- Fixed: a refresh that answered late could put older repository state back on screen, so after
  a commit the toolbar could keep offering to undo the step before it instead of the commit,
  until the next refresh. This happened when a read sent before the commit (another refresh, or
  the undo check when the window regains focus) answered after the commit's own refresh. Results
  from a read now only apply when no read sent after it has already landed (#342).
