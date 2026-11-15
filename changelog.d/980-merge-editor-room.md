### Merge editor

- Changed: while a conflicted file is open, the merge editor gets the room: the file list
  shrinks to the conflicts ("Show all changes" brings the rest back), the diff options are
  hidden, and the commit box is one line until every conflict is resolved (#980).
- Added: a pane too narrow for three panes shows two: ours and theirs side by side, each
  block's result under them (#980).
- Fixed: why the panes aren't available in a narrow pane is visible text next to the layout
  buttons, not only a tooltip (#980).
