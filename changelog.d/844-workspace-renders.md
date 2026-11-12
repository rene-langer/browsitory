### Faster with many branches

- Changed: an update in one part of a repository's window (opening the command palette, a toast,
  a refresh with nothing new) no longer draws the branch list, the commit graph, the worktree
  panel and the diff again, and the branch list does its per-row work once per update instead of
  once per row. With thousands of branches the sidebar stays responsive (#844).
