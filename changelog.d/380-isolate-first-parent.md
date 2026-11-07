### Isolate branch

- Changed: With a single branch in the graph (for example after **Isolate branch**), the graph
  shows only that branch's own line: its merge commits and the commits made directly on it, not
  every commit of the branches merged into it. A merge whose merged branch isn't shown gets a
  short dashed stub instead of a line running to the bottom of the graph. Commit search without
  `branch:` follows the same rule, so it finds what the graph shows (#380).
