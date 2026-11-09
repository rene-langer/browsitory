### Undo label of one-click history edits

- Fixed: the Undo entry of Reword, Squash, Fixup, Move up, Move down and Drop from the commit
  menu read "Rebase onto <id>"; it now names the edit, for example "Drop 1a2b3c4" or "Squash
  1a2b3c4 into its parent", also after a paused edit is continued (#473).
