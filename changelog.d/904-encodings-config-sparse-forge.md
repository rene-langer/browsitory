### Author names, settings changed outside, sparse checkouts and pull request cards

- Fixed: author and committer names stored in another encoding (`i18n.commitEncoding`, a
  repository converted from SVN or CVS) showed empty in the graph, blame and the rebase planner
  and garbled in the commit details and a squash merge's message; they are read as `git log`
  reads them, a search by author finds them, and a rebase, cherry-pick, fold or amend writes
  them in UTF-8 as git does (#1214).
- Fixed: `core.autocrlf`, `core.eol`, `core.safecrlf` and the other core settings changed with
  `git config` in a terminal while the repository was open didn't count until it was opened
  again; they now count from the next action (#904).
- Fixed: in a sparse checkout with patterns set by `git sparse-checkout set --no-cone`, the
  patterns are matched as git matches them, so Reset, Abort and Undo write and mark the files
  they bring in as git does. **Reset** (hard) and **Abort** of a rebase also apply the sparse
  checkout to every other file, as git does: an unmarked file outside it is marked and taken
  off the disk, a marked file inside it is written (#1187).
- Fixed: a GitHub or Bitbucket remote whose https URL holds a user name (Bitbucket's copy URL)
  got no pull request card; it gets one now, and the user name is neither shown nor sent. A URL
  with a password, or a GitHub token in place of the user name, still gets none (#1101).
