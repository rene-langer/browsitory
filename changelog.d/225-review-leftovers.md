### Security review leftovers

- Fixed: on Linux, a `core.ignorecase` that isn't true or false (or a config that can't be read)
  no longer counts as "case doesn't fold", which skipped the check for names that differ only in
  case. Adding a worktree, cloning, updating submodules, switching branches, merging and the
  other checks now stop with "Can't read core.ignorecase from the Git config: …"; cleaning up
  (Abort, Reset hard, discards) still runs its check as if case folds (#225).
- Internal: the commands refused while another window holds the undo history are listed once,
  over every command, so the wait and the refusal can't disagree (#225).
- Internal: Mark resolved records a link's own stat data, so status doesn't read the link again
  on every refresh (#225).
- Internal: comments and docs on stat data, the document extension list and the undo lock are
  corrected, and tests check status right after Mark resolved (#225).
