### Diff algorithm

- Added: Preferences → Diff → Diff algorithm chooses Default (Myers), Patience or Minimal for every
  diff. Staging, unstaging and discarding hunks or lines, and reverting or applying a commit's
  hunk, find the hunk with the algorithm the diff was shown with, so they act on the lines you
  see (#469).
- Added: "Toggle wrap lines" and "Toggle show whole file" in the command palette act on the
  current diff (#469).
