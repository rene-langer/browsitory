### Security review leftovers

- Fixed: a `core.ignorecase` that isn't true or false (or a config that can't be read) no
  longer counts as "case doesn't fold" on Linux, which skipped the check for names that differ
  only in case. Switching branches, merging, pulling, cherry-picking, rebasing, applying a stash,
  cloning, adding a worktree and updating submodules now stop with "Can't read core.ignorecase
  from the Git config: …". Every reset, discard, Abort and undo also stops, on every system,
  before it changes anything (the message names the setting). Fix the setting, then try again
  (#225).
- Internal: the commands refused while another window holds the undo history are listed once,
  over every command, so the wait and the refusal can't disagree (#225).
- Internal: Mark resolved records a link's own stat data, so status doesn't read the link again
  on every refresh (#225).
- Internal: comments and docs on stat data, the document extension list and the undo lock are
  corrected, and tests check status right after Mark resolved (#225).
