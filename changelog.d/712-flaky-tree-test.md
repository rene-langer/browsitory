### Internal: CommitTreeBrowser test flake

- Fixed: the `CommitTreeBrowser` line history test selected a line and read the button's label
  at once, but the effect that subscribes the button to selection changes mounts together with
  the file's lines and could still be pending, so "History of line 2 of README.txt" was
  sometimes missing. The test now waits for the label with `findByRole` (#712).
