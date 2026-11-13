### Merge editor

- Added: inside a conflict block the merge editor marks the words that differ between our line
  and their line, in the three panes and in the block view, as the diff does (#879).
- Changed: with the focus in an unchanged part of the three-pane result, F7 and Shift+F7, the
  Previous and Next conflict buttons and the command palette go to the conflict after or before
  it, not the one focused last (#879).
