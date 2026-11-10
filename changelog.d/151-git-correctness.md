### Git correctness

- Added: a rebase step waiting for the signing program shows "Waiting for the signing program…"
  with Cancel in the hook status strip, as a commit, cherry-pick or revert already did. Before,
  only the 120 s timeout ended it (#151).
- Fixed: a merge, cherry-pick, revert or rebase step that adds files below a folder link the
  current commit tracks now stops at git's directory/file conflict instead of refusing: the link
  is moved aside as `<name>~HEAD` and the files are written in a real folder, never through the
  link (#155).
- Fixed: committing a merge drops only the merged commits that are already in another parent's
  history, as `git commit` does, instead of keeping all of them or none. A `--no-ff` merge
  (fast-forward set to "never") keeps every parent (#227).
- Fixed: with `core.ignorecase` on and two files whose names differ only in case already
  committed, a merge failed with "1 conflict prevents checkout". It now says which two files
  are in the way. A clone of such a commit under a global `core.ignorecase` is refused before it
  checks anything out (#512).
