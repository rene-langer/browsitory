### Merge editor and stash buttons

- Added: the merge editor highlights our lines, their lines, the base and the unchanged lines
  like the diff, following its **Syntax highlighting** option; the result fields stay plain
  text (#763).
- Added: merge editor keys. With a conflict block focused, **1** to **5** take **Accept ours**,
  **Accept theirs**, **Ours then theirs**, **Theirs then ours** and **Accept base**; **F7** and
  **Shift+F7** go to the next and previous conflict. In a result field the digits type text:
  **Esc** goes back to the block. The keys are on the buttons, in the shortcut sheet's new
  "Merge editor" group, and in the command palette, which offers the moves and choices for the
  current conflict too (#764).
- Changed: the stash view's **Apply**, **Pop** and **Drop…** show why they are blocked in
  visible text tied to the button, and both Drop confirmations tie their reason to the confirm
  button (#772).
