### Working tree: file list and one diff viewer

- Changed: the working tree is a compact list of the changed files, with Changes and Staged
  always on screen, and one diff viewer for the current file, instead of a stack of every
  file's diff (#639). Each row has a status letter, the path, its menu and its stage button.
  When the current file is staged or discarded, the next one takes its place.
- Added: Down and Up move on into the other group past either end, Enter moves into the diff,
  and j and k in the diff go to the next and previous file (#639).
- Changed: with Expand untracked files off, an untracked file's diff loads when you press Show
  contents (#623, #639).
- Changed: each pane has one diff toolbar, with icon buttons and tooltips, that applies to every
  file in it and stays on one line, instead of a row of options above every file (#640).
- Changed: Split view and Show whole file are remembered, and Unified view and Split view are one
  pressed-state control; Wrap lines sets the Word wrap setting for every diff (#640).
