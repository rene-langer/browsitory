### Code clean-ups

- Internal: the path-limited stash is built in named steps (select and check the paths, build
  the index, untracked and worktree commits, push `refs/stash`, revert the paths) instead of one
  300-line function (#502).
