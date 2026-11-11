### Code clean-ups

- Internal: committing and a cherry-pick or revert step are built from named phases (merge state
  read, hooks, commit, clean-up) instead of one long function each (#502).
