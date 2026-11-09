### Command palette commit actions and the stash dialog

- Fixed: Cherry-pick and Revert from the command palette showed no "Cherry-picked" or "Reverted"
  toast, unlike the commit menu; they now run the menu's own action (#507).
- Fixed: the command palette's "Reset to …" and a merge commit's Cherry-pick and Revert did
  nothing while search results or a file's history replaced the graph; they now open the reset
  dialog or the parent picker directly (#507).
- Fixed: the Stash dialog still stashed when a rebase or a cherry-pick/revert sequence started
  while it was open (for example in a terminal); its Stash button is now disabled with the
  reason (#508).
