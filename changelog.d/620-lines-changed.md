### Lines changed per commit

- Added: each commit row in the history shows the lines the commit added and removed, for
  example `+12 −3` (green and red; read as "12 lines added, 3 removed"). They are counted as
  rows come into view, a batch at a time, without holding up the graph; a commit too large to
  count in time shows none. The ± button in the search bar above the graph hides them (#620).
- Added: the commit details show the totals ("4 files changed, +120 −37") and each file's
  header its own `+/−`, or "Binary" (#620).
