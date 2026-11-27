### Clone, pull requests, reflog and undo history

- Added: **Clear undo history…** in the command palette removes the open repository's undo and
  redo history and the Git references that keep its snapshots, after a confirmation. The
  privacy notice and the undo guide now say that those references hold copies of uncommitted,
  untracked and discarded files, and that a mirror push, a bundle of all refs or a mirror clone
  copies them (#1123).
- Fixed: Clone and Update submodule no longer crash when `merge.conflictstyle` is written
  without a value in your own, the system's or the submodule's Git config; they say which
  setting to fix and where, and change nothing. Other commands now name your global or the
  system's config when the setting is there (#1189).
- Fixed: **Cancel clone** stops a clone within a few seconds even while the server sends
  nothing (still connecting, preparing a big repository, or the network gone); what it
  downloaded is removed once its connection ends (#1159).
- Fixed: Listing pull requests no longer holds the repository's status, history and other
  commands while GitHub or Bitbucket doesn't answer, and creating one no longer holds the
  status and history; an unreachable forge fails after 10 s instead of 30 s (#1160).
- Fixed: The Reflog section reads the newest 200 entries, with **Load more** for older ones, and
  reads them again only when a branch, tag or HEAD moves, not after every staging click (#1161).
