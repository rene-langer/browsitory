### Merge editor

- Changed: while a conflicted file is open, the merge editor gets the room: the history column
  narrows (it comes back afterwards, and a drag meanwhile is kept), the file list shrinks to
  the conflicts ("Show all changes" brings the rest back), the diff options are hidden, and the
  commit box is one line until every conflict is resolved. The app now opens at 1280×800
  (was 1200×800), so the three panes show on a first launch (#980).
- Fixed: the shrunk file list opens at its top, with the conflicts in view (#980).
- Added: a pane too narrow for three panes shows two: ours and theirs side by side, each
  block's result under them (#980).
- Fixed: why the panes aren't available in a narrow pane is visible text next to the layout
  buttons, not only a tooltip (#980).
