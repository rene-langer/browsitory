### Commit flow and commit details

- Added: the commit button says what it commits and where, for example "Commit 3 files to main",
  and its arrow has "Commit and push" and "Amend last commit" (#651).
- Added: after a commit a notification names it ("Committed 4c5beaf to main") with Push, or with
  Undo when the repository has no remote (#651).
- Changed: an empty commit message now says why Commit is disabled (#651).
- Added: the commit details show the committer when someone else committed the change, the
  commit time when it differs from the author's, and the branches, remote branches and tags that
  contain the commit (#641).
- Changed: the commit details show dates as `2026-09-08 21:00`, the form the graph uses (#641).
