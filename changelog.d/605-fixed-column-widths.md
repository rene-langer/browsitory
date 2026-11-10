### Column widths

- Changed: the sidebar and the commit graph keep the width you set when the window is resized;
  only the diff pane grows or shrinks. When the window gets too narrow for both, the sidebar
  collapses instead of squeezing the graph, and it comes back at its width once there is room
  (#605).
- Fixed: expanding a collapsed sidebar by double-click or arrow key restored the default width
  instead of the width it had before (#605).
