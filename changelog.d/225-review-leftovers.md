### Security review leftovers

- Fixed: on Linux, a repository whose config can't be read, or whose `core.ignorecase` isn't a
  valid true/false value, no longer skips the check for names that differ only in case. Adding a
  worktree, cloning, updating submodules, switching branches, merging and other checkouts now
  stop with the config error instead (#225).
- Internal: the commands refused while another window holds the undo history are listed once,
  so the wait and the refusal can't disagree (#225).
- Internal: Mark resolved records a link's own stat data, so status doesn't read the link again
  on every refresh (#225).
- Internal: comments and docs on stat data, the document extension list and the undo lock are
  corrected, and tests check status right after Mark resolved (#225).
