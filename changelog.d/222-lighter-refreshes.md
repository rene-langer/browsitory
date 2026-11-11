### Lighter refreshes

- Changed: staging, unstaging, discarding or editing a file no longer reloads the history,
  branches, tags and upstreams; they are read again only when a commit, branch, tag, remote or
  the graph's selection changed, or on Refresh (#685).
- Fixed: actions and checks that ask for a refresh while one runs now share one that follows it,
  so a busy repository no longer works through a queue of outdated refreshes (#222).
