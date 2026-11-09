### Dialogs that start repository operations

- Fixed: the Reset dialog, the parent picker of a merge commit's Cherry-pick and Revert, the
  Merge dialog, "Push branch to…", "Delete remote branch…" and the force push confirmation still
  ran their operation when the repository became busy while they were open (for example a rebase
  started in a terminal, picked up by a refresh); their confirm button is now disabled with the
  reason (#514).
