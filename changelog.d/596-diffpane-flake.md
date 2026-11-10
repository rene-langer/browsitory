### Internal: DiffPane test flake

- Fixed: three `DiffPane` diff-options tests selected a line right after a hunk action, while
  the refetch that action starts could still land and clear the line selection, so "Stage lines
  (1)" was sometimes missing. They now wait for that refetch to land before selecting a line, and
  query the button with `findByRole` (#596).
