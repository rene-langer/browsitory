### Reliability

- Fixed: a mixed or hard reset writes the index before it moves the branch, as git does. HEAD
  no longer points at the target while the index still holds the old tree, and a reset whose
  index can't be written (another git command holds `index.lock`) now leaves HEAD where it was
  instead of moving it (#796).
- Internal: a file watcher marks itself stopped before it reports `Stopped`, so a watch
  requested right after the event starts a new watcher; this also fixes the flaky
  `growing_past_the_folder_limit_stops_the_watcher` test (#797).
- Internal: CI's mutation testing comment says "Mutation testing didn't run" when an earlier
  step failed or cargo-mutants left no results, instead of claiming every mutant was caught
  (#798).
- Internal: the blame E2E spec looks the commit message field up again before it types into
  it, and a failed commit there records the commit box's state and a screenshot (#810).
