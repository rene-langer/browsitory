### Code clean-ups

- Internal: the git-core tests share `git_ok`, `head_id` and `index_paths` from
  `tests/common` instead of keeping their own copies (#505).
- Internal: stash, conflict resolution and rebase write a work-tree file's blob through git's
  clean filters with one shared helper (#210).
