### Faster refresh of a long graph

- Changed: after a commit, a fetch or a branch switch, the graph no longer reads and sends every
  row already loaded again: the worker sends only the new rows, which loaded rows stay, and the
  branch and tag badges that moved. With thousands of rows loaded a refresh costs about one page
  (#835).
