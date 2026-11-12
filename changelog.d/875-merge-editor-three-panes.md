### Merge editor

- Added: the merge editor shows the whole conflicted file in three panes, ours, the result and
  theirs, each with its own line numbers; the unchanged lines stay level across the panes, which
  scroll together, and every block keeps its choices, keys and editable result. **Blocks**
  switches back to one block per conflict, and the editor remembers the layout; a pane too narrow
  for three (the VS Code side panel) shows the blocks (#875).
